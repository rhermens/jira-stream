use gouqi::{Issue, r#async::Jira};

use crate::socket::EventLog;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct State {
    pub issue: Issue,
    pub parent: Option<Issue>,
}

impl State {
    pub async fn fetch(jira: &Jira, ticket: &str) -> gouqi::Result<Self> {
        let issue = jira.issues().get(ticket).await?;
        let parent = match issue
            .fields
            .get("parent")
            .and_then(|parent| parent.get("key"))
            .and_then(serde_json::Value::as_str)
        {
            Some(key) => Some(jira.issues().get(key).await?),
            None => None,
        };

        Ok(Self { issue, parent })
    }

    pub fn maybe_update(&mut self, next: Self, events: Option<&EventLog>) {
        for issue in next.changed_issues(self) {
            emit_issue(issue, events);
        }
        *self = next;
    }

    pub fn emit(&self, events: Option<&EventLog>) {
        for issue in self.issues() {
            emit_issue(issue, events);
        }
    }

    fn issues(&self) -> impl Iterator<Item = &Issue> {
        std::iter::once(&self.issue).chain(self.parent.iter())
    }

    fn changed_issues<'a>(&'a self, previous: &'a Self) -> impl Iterator<Item = &'a Issue> {
        self.issues().filter(|issue| {
            previous
                .issues()
                .find(|old| old.key == issue.key)
                .is_none_or(|old| old.fields.get("updated") != issue.fields.get("updated"))
        })
    }
}

fn emit_issue(issue: &Issue, events: Option<&EventLog>) {
    let json = serde_json::to_string(issue).expect("Failed to serialize issue");
    println!("{json}");
    if let Some(events) = events {
        events.push(format!("{json}\n"));
    }
}
