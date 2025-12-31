use std::str::FromStr;

use colored::Colorize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Update {
    pub html_url: String,
    pub name: String,
}

const UPDATE_URL: &str = "https://api.github.com/repos/Neuron-Mr-White/neutun/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn check() {
    match check_inner().await {
        Ok(Some(new)) => {
            eprintln!(
                "{} {} => {} ({})\n",
                "New version available:".yellow().italic(),
                CURRENT_VERSION.bright_blue(),
                new.name.as_str().green(),
                new.html_url
            );
        }
        Ok(None) => log::debug!("Using latest version."),
        Err(error) => log::error!("Failed to check version: {:?}", error),
    }
}

/// checks for a new release on github
async fn check_inner() -> Result<Option<Update>, Box<dyn std::error::Error>> {
    let response = reqwest::Client::new()
        .get(UPDATE_URL)
        .header("User-Agent", "neutun-client")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    let update: Update = response.json().await?;

    let cur = semver::Version::from_str(CURRENT_VERSION)?;
    let remote_version_str = update.name.trim_start_matches('v');
    let remote = semver::Version::from_str(remote_version_str)?;

    if remote > cur {
        Ok(Some(update))
    } else {
        Ok(None)
    }
}
