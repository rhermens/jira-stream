use std::env;

use clap::Parser;
use gouqi::{Credentials, r#async::Jira};

#[derive(Debug, clap::Parser)]
struct Args {
    #[arg(short, long, default_value_t = 30)]
    pub interval: u64,

    #[arg(short, long)]
    pub username: String,

    #[arg(short, long)]
    pub token: Option<String>,

    #[arg(long)]
    pub host: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let jira_token = args
        .token
        .or(env::var("JIRA_TOKEN").ok())
        .expect("No jira auth");
    let credentials = Credentials::Basic(args.username, jira_token);
    let jira = Jira::new(args.host, credentials).expect("No jira");

    let session = jira.session().await?;

    dbg!(session);

    Ok(())
}
