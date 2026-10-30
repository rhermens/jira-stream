use std::{env, fs, io, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub username: String,
    pub host: String,
    pub token: String,
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        let config_dir = env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| env::var("HOME").map(|home| PathBuf::from(home).join(".config")))
            .ok()?;

        Some(config_dir.join("jira-stream/jira-stream.yaml"))
    }

    pub fn try_load() -> Option<Config> {
        let contents = fs::read_to_string(Self::path()?).ok()?;

        serde_yaml::from_str(&contents).ok()
    }

    pub fn save(&self) -> io::Result<PathBuf> {
        let path = Self::path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No config directory"))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_yaml::to_string(self).map_err(io::Error::other)?;
        fs::write(&path, contents)?;

        Ok(path)
    }
}
