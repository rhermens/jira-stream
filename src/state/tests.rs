use super::*;
use gouqi::Credentials;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
    time::{Duration, timeout},
};

fn issue(key: &str, updated: &str) -> Issue {
    serde_json::from_value(json!({
        "self": format!("https://jira.example/rest/api/2/issue/{key}"),
        "id": key,
        "key": key,
        "fields": { "updated": updated }
    }))
    .unwrap()
}

fn state(updated: &str, parent: Option<(&str, &str)>) -> State {
    State {
        issue: issue("TEST-1", updated),
        parent: parent.map(|(key, updated)| issue(key, updated)),
    }
}

fn changed_keys<'a>(next: &'a State, previous: &'a State) -> Vec<&'a str> {
    next.changed_issues(previous)
        .map(|issue| issue.key.as_str())
        .collect()
}

#[test]
fn unchanged_issues_are_not_emitted() {
    let previous = state("1", Some(("TEST-2", "1")));
    let next = state("1", Some(("TEST-2", "1")));
    assert!(changed_keys(&next, &previous).is_empty());
}

#[test]
fn child_changes_are_emitted_independently() {
    let previous = state("1", Some(("TEST-2", "1")));
    let next = state("2", Some(("TEST-2", "1")));
    assert_eq!(changed_keys(&next, &previous), ["TEST-1"]);
}

#[test]
fn parent_changes_are_emitted_when_child_is_unchanged() {
    let previous = state("1", Some(("TEST-2", "1")));
    let next = state("1", Some(("TEST-2", "2")));
    assert_eq!(changed_keys(&next, &previous), ["TEST-2"]);
}

#[test]
fn simultaneous_changes_emit_child_then_parent() {
    let previous = state("1", Some(("TEST-2", "1")));
    let next = state("2", Some(("TEST-2", "2")));
    assert_eq!(changed_keys(&next, &previous), ["TEST-1", "TEST-2"]);
}

#[test]
fn new_parent_is_emitted_even_with_the_same_timestamp() {
    let previous = state("1", Some(("TEST-2", "1")));
    let next = state("1", Some(("TEST-3", "1")));
    assert_eq!(changed_keys(&next, &previous), ["TEST-3"]);
}

#[test]
fn added_parent_is_emitted() {
    let previous = state("1", None);
    let next = state("1", Some(("TEST-2", "1")));
    assert_eq!(changed_keys(&next, &previous), ["TEST-2"]);
}

#[test]
fn removed_parent_is_no_longer_watched() {
    let mut current = state("1", Some(("TEST-2", "1")));
    current.maybe_update(state("2", None));
    assert!(current.parent.is_none());
    assert_eq!(current.issue.fields["updated"], "2");
    assert!(changed_keys(&state("2", None), &current).is_empty());
}

#[test]
fn issues_without_parents_still_emit_changes() {
    let previous = state("1", None);
    let next = state("2", None);
    assert_eq!(changed_keys(&next, &previous), ["TEST-1"]);
}

async fn mock_jira(responses: Vec<(&'static str, u16, Value)>) -> (Jira, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        timeout(Duration::from_secs(5), async move {
            for (key, status, body) in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let byte = socket.read_u8().await.unwrap();
                    request.push(byte);
                    assert!(request.len() < 16_384);
                }
                let request = String::from_utf8(request).unwrap();
                let path = request.lines().next().unwrap().split_whitespace().nth(1).unwrap();
                assert!(path.ends_with(&format!("/issue/{key}")), "{path}");
                let body = body.to_string();
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        })
        .await
        .expect("Mock Jira timed out");
    });
    (Jira::new(host, Credentials::Anonymous).unwrap(), server)
}

async fn fetch(jira: &Jira) -> gouqi::Result<State> {
    timeout(Duration::from_secs(5), State::fetch(jira, "TEST-1"))
        .await
        .expect("Issue fetch timed out")
}

#[tokio::test]
async fn fetches_full_direct_parent_without_fetching_grandparent() {
    let mut child = serde_json::to_value(issue("TEST-1", "1")).unwrap();
    child["fields"]["parent"] = json!({ "key": "TEST-2" });
    let mut parent = serde_json::to_value(issue("TEST-2", "1")).unwrap();
    parent["fields"]["summary"] = json!("Full parent issue");
    parent["fields"]["parent"] = json!({ "key": "TEST-3" });
    let (jira, server) = mock_jira(vec![
        ("TEST-1", 200, child),
        ("TEST-2", 200, parent.clone()),
    ])
    .await;

    let result = fetch(&jira).await.unwrap();
    server.await.unwrap();
    assert_eq!(result.issue.key, "TEST-1");
    assert_eq!(
        serde_json::to_value(result.parent.unwrap()).unwrap(),
        parent
    );
}

#[tokio::test]
async fn fetches_issues_with_missing_or_null_parent() {
    for parent in [None, Some(Value::Null)] {
        let mut child = serde_json::to_value(issue("TEST-1", "1")).unwrap();
        if let Some(parent) = parent {
            child["fields"]["parent"] = parent;
        }
        let (jira, server) = mock_jira(vec![("TEST-1", 200, child)]).await;
        assert!(fetch(&jira).await.unwrap().parent.is_none());
        server.await.unwrap();
    }
}

#[tokio::test]
async fn refetches_parent_and_follows_reparenting() {
    let mut child = serde_json::to_value(issue("TEST-1", "1")).unwrap();
    child["fields"]["parent"] = json!({ "key": "TEST-2" });
    let mut reparented = child.clone();
    reparented["fields"]["parent"] = json!({ "key": "TEST-3" });
    let (jira, server) = mock_jira(vec![
        ("TEST-1", 200, child.clone()),
        (
            "TEST-2",
            200,
            serde_json::to_value(issue("TEST-2", "1")).unwrap(),
        ),
        ("TEST-1", 200, child),
        (
            "TEST-2",
            200,
            serde_json::to_value(issue("TEST-2", "2")).unwrap(),
        ),
        ("TEST-1", 200, reparented),
        (
            "TEST-3",
            200,
            serde_json::to_value(issue("TEST-3", "2")).unwrap(),
        ),
    ])
    .await;

    let mut current = fetch(&jira).await.unwrap();
    let next = fetch(&jira).await.unwrap();
    assert_eq!(changed_keys(&next, &current), ["TEST-2"]);
    current.maybe_update(next);
    assert_eq!(current.parent.as_ref().unwrap().fields["updated"], "2");
    let next = fetch(&jira).await.unwrap();
    assert_eq!(changed_keys(&next, &current), ["TEST-3"]);
    current.maybe_update(next);
    assert_eq!(current.parent.as_ref().unwrap().key, "TEST-3");
    server.await.unwrap();
}

#[tokio::test]
async fn propagates_parent_fetch_errors() {
    let mut child = serde_json::to_value(issue("TEST-1", "1")).unwrap();
    child["fields"]["parent"] = json!({ "key": "TEST-2" });
    let (jira, server) = mock_jira(vec![
        ("TEST-1", 200, child),
        ("TEST-2", 404, json!({ "errorMessages": ["Not found"] })),
    ])
    .await;

    assert!(fetch(&jira).await.is_err());
    server.await.unwrap();
}
