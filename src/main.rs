use std::{env, time::Duration};

use clap::Parser;
use gouqi::{Credentials, Issue, r#async::Jira};
use jira_stream::config::AcliProfile;

#[derive(Debug, clap::Parser)]
struct Args {
    #[arg(short, long, default_value_t = 30)]
    pub interval: u64,

    #[arg(short, long)]
    pub username: Option<String>,

    #[arg(short, long)]
    pub token: Option<String>,

    #[arg(long)]
    pub host: Option<String>,

    #[arg(long)]
    pub ticket: String,
}

#[derive(Debug)]
struct State {
    pub issue: Issue,
}

impl State {
    pub fn new(issue: Issue) -> Self {
        let ret = Self { issue };
        ret.emit();
        ret
    }

    pub fn maybe_update(&mut self, issue: Issue) {
        if self.issue.fields.get("updated") == issue.fields.get("updated") {
            return;
        }

        self.issue = issue;
        self.emit();
    }

    pub fn emit(&self) {
        println!(
            "{}",
            serde_json::to_string(&self.issue).expect("Failed to serialize issue")
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let profile = AcliProfile::try_new();

    let username = args
        .username
        .or_else(|| profile.as_ref().map(|p| p.email.clone()))
        .expect("No jira username");
    let host = args
        .host
        .or_else(|| profile.as_ref().map(|p| format!("https://{}", p.site)))
        .expect("No jira host");
    let jira_token = args
        .token
        .or(env::var("JIRA_TOKEN").ok())
        .expect("No jira auth");
    let credentials = Credentials::Basic(username, jira_token);
    let jira = Jira::new(host, credentials).expect("No jira");

    let issue = jira.issues().get(&args.ticket).await?;
    let mut state = State::new(issue);

    loop {
        tokio::time::sleep(Duration::from_secs(args.interval)).await;
        let issue = jira.issues().get(&args.ticket).await?;
        state.maybe_update(issue);
    }
}
