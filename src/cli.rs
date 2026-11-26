use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Fetch and watch Jira issues as newline-delimited JSON.
///
/// Fetch or watch one issue and its direct parent, when present.
/// Each issue is written as a separate JSON line, with the requested issue first.
/// Ancestors beyond the direct parent are not fetched.
///
/// Run `jira-stream login` before fetching or watching issues.
#[derive(Debug, Parser)]
#[command(
    name = "jira-stream",
    version,
    after_long_help = "Examples:\n  jira-stream login\n  jira-stream fetch --ticket TEST-123\n  jira-stream watch --ticket TEST-123 --interval 60"
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Fetch an issue and its direct parent once.
    ///
    /// Write the full issue and its direct parent, when present, as separate JSON
    /// lines to standard output, then exit. Uses credentials saved by
    /// `jira-stream login`.
    Fetch {
        /// Jira issue key to fetch, for example TEST-123.
        #[arg(long, value_name = "KEY")]
        ticket: String,
    },
    /// Watch an issue and its direct parent for changes.
    ///
    /// Emit the initial issue and its direct parent as separate JSON lines.
    /// Poll both issues and emit each independently when its updated field changes.
    /// Automatically follow a new direct parent when the issue is reparented.
    /// Uses credentials saved by `jira-stream login`. Stop watching with Ctrl+C.
    Watch {
        /// Polling interval in seconds.
        ///
        /// Wait this many seconds after each fetch before checking the issue and
        /// its direct parent again.
        #[arg(short, long, default_value_t = 30, value_name = "SECONDS")]
        interval: u64,

        /// Jira issue key to watch, for example TEST-123.
        #[arg(long, value_name = "KEY")]
        ticket: String,

        /// Replay session history, then stream JSON events through this Unix socket.
        ///
        /// Standard output remains enabled. Parent directories are created as needed.
        /// Each connection receives all events emitted since startup, then live events.
        /// History is retained in memory until exit.
        /// The socket is removed on Ctrl-C, SIGTERM, SIGHUP, and normal exit.
        /// After SIGKILL, remove the leftover socket before restarting.
        #[arg(long, value_name = "PATH")]
        socket: Option<PathBuf>,
    },
    /// Save Jira credentials interactively.
    ///
    /// Prompt for a username (email), Jira host URL (for example
    /// https://example.atlassian.net), and API token.
    /// Save these values as plain-text YAML at
    /// $XDG_CONFIG_HOME/jira-stream/jira-stream.yaml, or
    /// $HOME/.config/jira-stream/jira-stream.yaml when XDG_CONFIG_HOME is unset.
    /// The API token is visible while typing.
    Login,
}
