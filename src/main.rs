use std::{
    io::{self, Write},
    time::Duration,
};

use clap::Parser;
use gouqi::{Credentials, r#async::Jira};
use jira_stream::{
    config::Config,
    socket::{EventLog, SocketServer},
    state::State,
};

mod cli;

use cli::{CliArgs, Command};

#[tokio::main]
async fn main() {
    let args = CliArgs::parse();

    match args.command {
        Command::Fetch { ticket } => {
            let config = Config::try_load().expect("No config found, run login first");
            let credentials = Credentials::Basic(config.username, config.token);
            let jira = Jira::new(config.host, credentials).expect("No jira");

            let state = State::fetch(&jira, &ticket)
                .await
                .expect("Failed to get issue or parent");
            state.emit(None);
        }
        Command::Watch {
            interval,
            ticket,
            socket,
        } => {
            let config = Config::try_load().expect("No config found, run login first");
            let credentials = Credentials::Basic(config.username, config.token);
            let jira = Jira::new(config.host, credentials).expect("No jira");

            let socket = socket
                .as_deref()
                .map(SocketServer::bind)
                .transpose()
                .expect("Failed to bind socket");
            let events = socket.as_ref().map(SocketServer::events);

            // Return on Ctrl-C so the socket server drops and removes its path.
            tokio::select! {
                () = watch(&jira, &ticket, interval, events.as_ref()) => {}
                result = tokio::signal::ctrl_c() => result.expect("Failed to listen for Ctrl-C"),
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

async fn watch(jira: &Jira, ticket: &str, interval: u64, events: Option<&EventLog>) {
    let mut state = State::fetch(jira, ticket)
        .await
        .expect("Failed to get issue or parent");
    state.emit(events);

    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        let next = State::fetch(jira, ticket)
            .await
            .expect("Failed to get issue or parent");
        state.maybe_update(next, events);
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
