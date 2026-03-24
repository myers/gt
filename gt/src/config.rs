use eyre::{Result, WrapErr};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    default: ConfigProfile,
}

#[derive(Debug, Deserialize, Default)]
struct ConfigProfile {
    url: Option<String>,
    token: Option<String>,
}

#[derive(Debug)]
pub struct Config {
    pub url: url::Url,
    pub token: String,
}

impl Config {
    /// Load config from file and env vars (env vars take precedence).
    pub fn load() -> Result<Self> {
        let file_config = load_config_file().unwrap_or_default();

        let url_str = std::env::var("GITEA_URL")
            .ok()
            .or(file_config.default.url)
            .ok_or_else(|| {
                eyre::eyre!(
                    "No Gitea URL configured. Set GITEA_URL or add url to ~/.config/gt/config.toml"
                )
            })?;

        let token = std::env::var("GITEA_TOKEN")
            .ok()
            .or(file_config.default.token)
            .ok_or_else(|| {
                eyre::eyre!(
                    "No Gitea token configured. Set GITEA_TOKEN or add token to ~/.config/gt/config.toml"
                )
            })?;

        let url = url::Url::parse(&url_str)
            .wrap_err_with(|| format!("Invalid URL: {url_str}"))?;

        Ok(Config { url, token })
    }

    /// Create a Gitea API client from this config.
    pub fn client(&self) -> Result<gitea_api::Gitea> {
        gitea_api::Gitea::new(
            gitea_api::Auth::Token(&self.token),
            self.url.clone(),
        )
        .map_err(|e| eyre::eyre!("{e}"))
    }
}

fn load_config_file() -> Option<ConfigFile> {
    let path = config_path()?;
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

fn config_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "gt")?;
    let path = dirs.config_dir().join("config.toml");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}
