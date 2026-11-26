use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

struct Watcher {
    child: Child,
    directory: PathBuf,
    socket: PathBuf,
    _jira: TcpListener,
}

impl Watcher {
    fn start() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "js-shutdown-{}-{}",
            std::process::id(),
            NEXT_PATH.fetch_add(1, Ordering::Relaxed)
        ));
        let config_directory = directory.join("jira-stream");
        fs::create_dir_all(&config_directory).unwrap();
        // Keep the initial fetch pending locally, without contacting Jira.
        let jira = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = jira_stream::config::Config {
            host: format!("http://{}", jira.local_addr().unwrap()),
            username: "test@example.com".into(),
            token: "test-token".into(),
        };
        fs::write(
            config_directory.join("jira-stream.yaml"),
            serde_yaml::to_string(&config).unwrap(),
        )
        .unwrap();
        let socket = directory.join("events.sock");
        let child = Command::new(env!("CARGO_BIN_EXE_jira-stream"))
            .args(["watch", "--ticket", "TEST-123", "--socket"])
            .arg(&socket)
            .env("XDG_CONFIG_HOME", &directory)
            .env("NO_PROXY", "127.0.0.1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut watcher = Self {
            child,
            directory,
            socket,
            _jira: jira,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !watcher.socket.exists() {
            assert!(watcher.child.try_wait().unwrap().is_none(), "early exit");
            assert!(Instant::now() < deadline, "socket was not created");
            thread::sleep(Duration::from_millis(10));
        }
        watcher
    }

    fn signal_and_wait(&mut self, signal: &str) {
        assert!(
            self.child.try_wait().unwrap().is_none(),
            "exited before signal"
        );
        assert!(
            Command::new("kill")
                .args([signal, &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(!self.socket.exists(), "socket remains after {signal}");
                assert!(status.success(), "watcher exited with {status}");
                break;
            }
            assert!(Instant::now() < deadline, "watcher did not exit");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn removes_socket_on_sigterm() {
    Watcher::start().signal_and_wait("-TERM");
}

#[test]
fn removes_socket_on_sigint() {
    Watcher::start().signal_and_wait("-INT");
}

#[test]
fn removes_socket_on_sighup() {
    Watcher::start().signal_and_wait("-HUP");
}
