use std::{env, fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {}

#[derive(Debug, Deserialize)]
pub struct AcliProfile {
    pub site: String,
    pub cloud_id: String,
    pub account_id: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
struct AcliConfig {
    current_profile: String,
    profiles: Vec<AcliProfile>,
}

impl AcliProfile {
    pub fn try_new() -> Option<AcliProfile> {
        let config_dir = env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| env::var("HOME").map(|home| PathBuf::from(home).join(".config")))
            .ok()?;
        let contents = fs::read_to_string(config_dir.join("acli/jira_config.yaml")).ok()?;
        let config: AcliConfig = serde_yaml::from_str(&contents).ok()?;

        config
            .profiles
            .into_iter()
            .find(|p| format!("{}:{}", p.cloud_id, p.account_id) == config.current_profile)
    }
}
