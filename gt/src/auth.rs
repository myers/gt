use clap::{Args, Subcommand};
use eyre::Result;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Args)]
pub struct AuthCommand {
    #[command(subcommand)]
    action: AuthAction,
}

#[derive(Subcommand)]
enum AuthAction {
    /// Log in to a Gitea instance
    Login(LoginArgs),
    /// Show current authentication status
    Status,
    /// Log out (remove config file)
    Logout,
}

#[derive(Args)]
struct LoginArgs {
    /// Gitea instance URL
    #[arg(long)]
    url: Option<String>,

    /// API token
    #[arg(long)]
    token: Option<String>,
}

impl AuthCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            AuthAction::Login(args) => login(args),
            AuthAction::Status => status(),
            AuthAction::Logout => logout(),
        }
    }
}

fn config_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "gt")
        .ok_or_else(|| eyre::eyre!("Could not determine config directory"))?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn login(args: &LoginArgs) -> Result<()> {
    let url = match &args.url {
        Some(u) => u.clone(),
        None => {
            eprint!("Gitea URL: ");
            io::stderr().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        }
    };

    let token = match &args.token {
        Some(t) => t.clone(),
        None => {
            eprint!("API token: ");
            io::stderr().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        }
    };

    if url.is_empty() || token.is_empty() {
        eyre::bail!("URL and token are required");
    }

    // Validate URL
    url::Url::parse(&url).map_err(|e| eyre::eyre!("Invalid URL: {e}"))?;

    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = format!(
        "[default]\nurl = \"{url}\"\ntoken = \"{token}\"\n"
    );
    std::fs::write(&path, content)?;

    eprintln!("Logged in to {url}");
    eprintln!("Config saved to {}", path.display());
    Ok(())
}

fn status() -> Result<()> {
    let path = config_path()?;

    if !path.exists() {
        eprintln!("Not logged in (no config file at {})", path.display());
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let config: toml::Value = toml::from_str(&content)?;

    let url = config
        .get("default")
        .and_then(|d| d.get("url"))
        .and_then(|u| u.as_str())
        .unwrap_or("(not set)");

    let token = config
        .get("default")
        .and_then(|d| d.get("token"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let token_preview = if token.len() > 8 {
        format!("{}...{}", &token[..4], &token[token.len() - 4..])
    } else if !token.is_empty() {
        "****".to_string()
    } else {
        "(not set)".to_string()
    };

    println!("URL:   {url}");
    println!("Token: {token_preview}");
    println!("Config: {}", path.display());
    Ok(())
}

fn logout() -> Result<()> {
    let path = config_path()?;

    if !path.exists() {
        eprintln!("Already logged out (no config file)");
        return Ok(());
    }

    std::fs::remove_file(&path)?;
    eprintln!("Logged out (removed {})", path.display());
    Ok(())
}
