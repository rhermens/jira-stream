use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    io::AsyncWriteExt,
    net::{UnixListener, UnixStream},
    sync::watch,
    task::{JoinHandle, JoinSet},
};

const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Shared, append-only history retained for the lifetime of the watcher.
#[derive(Clone, Debug)]
pub struct EventLog {
    history: Arc<Mutex<Vec<Arc<str>>>>,
    changed: watch::Sender<()>,
}

impl EventLog {
    fn new() -> Self {
        let (changed, _) = watch::channel(());
        Self {
            history: Arc::new(Mutex::new(Vec::new())),
            changed,
        }
    }

    pub fn push(&self, event: String) {
        self.history
            .lock()
            .expect("Event history poisoned")
            .push(event.into());
        self.changed.send_replace(());
    }

    fn get(&self, index: usize) -> Option<Arc<str>> {
        self.history
            .lock()
            .expect("Event history poisoned")
            .get(index)
            .cloned()
    }
}

struct SocketPath(PathBuf);

impl Drop for SocketPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Replays session history, then streams new JSON lines without blocking polling.
pub struct SocketServer {
    events: EventLog,
    task: JoinHandle<()>,
    _path: SocketPath,
}

impl SocketServer {
    pub fn bind(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        // Binding fails if the path exists; never remove another server's socket.
        let listener = UnixListener::bind(path)?;
        let path = SocketPath(path.to_owned());
        fs::set_permissions(&path.0, fs::Permissions::from_mode(0o600))?;
        let events = EventLog::new();
        let client_events = events.clone();
        let task = tokio::spawn(async move {
            let mut clients = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((client, _)) => {
                                clients.spawn(send_events(client, client_events.clone()));
                            }
                            Err(error) => {
                                eprintln!("Unable to accept socket client: {error}");
                                break;
                            }
                        }
                    }
                    _ = clients.join_next(), if !clients.is_empty() => {}
                }
            }
        });
        Ok(Self {
            events,
            task,
            _path: path,
        })
    }

    pub fn events(&self) -> EventLog {
        self.events.clone()
    }
}

impl Drop for SocketServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn send_events(mut client: UnixStream, events: EventLog) {
    // Subscribe before reading history so appends cannot be missed at the live boundary.
    let mut changed = events.changed.subscribe();
    let mut cursor = 0;
    loop {
        if let Some(event) = events.get(cursor) {
            if !matches!(
                tokio::time::timeout(WRITE_TIMEOUT, client.write_all(event.as_bytes())).await,
                Ok(Ok(()))
            ) {
                break;
            }
            cursor += 1;
        } else if changed.changed().await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::io::{AsyncBufReadExt, BufReader};

    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "jira-stream-test-{}-{}",
                std::process::id(),
                NEXT_PATH.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    async fn wait_for_clients(events: &EventLog, count: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while events.changed.receiver_count() != count {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Socket clients were not accepted");
    }

    #[tokio::test]
    async fn broadcasts_json_lines_to_multiple_clients() {
        let directory = TestDirectory::new();
        let path = directory.0.join("nested/events.sock");
        let server = SocketServer::bind(&path).unwrap();
        let sender = server.events();
        let mut first = BufReader::new(UnixStream::connect(&path).await.unwrap()).lines();
        let mut second = BufReader::new(UnixStream::connect(&path).await.unwrap()).lines();
        wait_for_clients(&sender, 2).await;

        let event = "{\"type\":\"test\",\"data\":\"hello\"}";
        sender.push(format!("{event}\n"));
        for client in [&mut first, &mut second] {
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), client.next_line())
                    .await
                    .unwrap()
                    .unwrap()
                    .as_deref(),
                Some(event)
            );
        }
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(server);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn clients_receive_session_history_before_live_events() {
        let directory = TestDirectory::new();
        let path = directory.0.join("events.sock");
        let server = SocketServer::bind(&path).unwrap();
        let sender = server.events();
        sender.push("old\n".into());
        let mut client = BufReader::new(UnixStream::connect(&path).await.unwrap()).lines();
        wait_for_clients(&sender, 1).await;
        sender.push("new\n".into());
        for expected in ["old", "new"] {
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), client.next_line())
                    .await
                    .unwrap()
                    .unwrap()
                    .as_deref(),
                Some(expected)
            );
        }
    }

    #[tokio::test]
    async fn replays_large_history_and_concurrent_appends_on_reconnect() {
        let directory = TestDirectory::new();
        let path = directory.0.join("events.sock");
        let server = SocketServer::bind(&path).unwrap();
        let events = server.events();
        for index in 0..256 {
            events.push(format!("{index}\n"));
        }
        let mut client = BufReader::new(UnixStream::connect(&path).await.unwrap()).lines();
        wait_for_clients(&events, 1).await;
        for index in 256..512 {
            events.push(format!("{index}\n"));
        }

        for connection in 0..2 {
            for index in 0..512 {
                let line = tokio::time::timeout(Duration::from_secs(2), client.next_line())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                assert_eq!(line, index.to_string());
            }
            if connection == 0 {
                // A new connection starts at the beginning of the same session history.
                client = BufReader::new(UnixStream::connect(&path).await.unwrap()).lines();
            }
        }
        events.push("live\n".into());
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), client.next_line())
                .await
                .unwrap()
                .unwrap()
                .as_deref(),
            Some("live")
        );
    }

    #[tokio::test]
    async fn refuses_existing_paths_without_removing_them() {
        let directory = TestDirectory::new();
        let file = directory.0.join("existing");
        fs::write(&file, "keep").unwrap();
        assert!(SocketServer::bind(&file).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "keep");

        let path = directory.0.join("events.sock");
        let _server = SocketServer::bind(&path).unwrap();
        assert!(SocketServer::bind(&path).is_err());
        assert!(UnixStream::connect(&path).await.is_ok());
    }
}
