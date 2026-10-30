use std::{
    io::{self, Write},
    time::Duration,
};

use clap::{Parser, Subcommand};
use gouqi::{Credentials, Issue, r#async::Jira};
use jira_stream::config::Config;

#[derive(Debug, clap::Parser)]
struct Args {
    #[arg(short, long)]
    pub username: Option<String>,

    #[arg(short, long)]
    pub token: Option<String>,

    #[arg(long)]
    pub host: Option<String>,

    #[arg(short, long, default_value_t = 30)]
    pub interval: u64,

    #[arg(long)]
    pub ticket: String,
}

#[derive(Debug, clap::Parser)]
struct CliArgs {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Watch {
        #[arg(short, long, default_value_t = 30)]
        interval: u64,

        #[arg(long)]
        ticket: String,
    },
    Login,
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
async fn main() {
    let args = CliArgs::parse();

    match args.command {
        Command::Watch { interval, ticket } => {
            let config = Config::try_load().expect("No config found, run login first");

            let credentials = Credentials::Basic(config.username, config.token);
            let jira = Jira::new(config.host, credentials).expect("No jira");

            let issue = jira
                .issues()
                .get(&ticket)
                .await
                .expect("Failed to get issue");
            let mut state = State::new(issue);

            loop {
                tokio::time::sleep(Duration::from_secs(interval)).await;
                let issue = jira
                    .issues()
                    .get(&ticket)
                    .await
                    .expect("Failed to get issue");
                state.maybe_update(issue);
            }
        }
        Command::Login => {
            let config = Config {
                username: prompt("Username (email)"),
                host: prompt("Host (e.g. https://example.atlassian.net)"),
                token: prompt("API token"),
            };
            let path = config.save().expect("Failed to save config");
            println!("Config saved to {}", path.display());
        }
    }
}

fn prompt(label: &str) -> String {
    print!("{label}: ");
    io::stdout().flush().expect("Failed to flush stdout");

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read input");
    input.trim().to_string()
}
