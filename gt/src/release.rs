use clap::{Args, Subcommand};
use eyre::Result;

use crate::config::Config;
use crate::issues::{atty_check, relative_time};
use crate::repo;

#[derive(Args)]
pub struct ReleaseCommand {
    #[command(subcommand)]
    action: ReleaseAction,
}

#[derive(Subcommand)]
enum ReleaseAction {
    /// List releases
    List(ListArgs),
    /// Create a release
    Create(CreateArgs),
    /// View a release
    View(ViewArgs),
    /// Download release assets
    Download(DownloadArgs),
    /// Delete a release
    Delete(DeleteArgs),
}

#[derive(Args)]
struct ListArgs {
    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct CreateArgs {
    /// Tag name
    #[arg(short, long)]
    tag: String,

    /// Release title
    #[arg(short, long)]
    name: String,

    /// Release body/notes
    #[arg(short, long)]
    body: Option<String>,

    /// Mark as draft
    #[arg(long)]
    draft: bool,

    /// Mark as prerelease
    #[arg(long)]
    prerelease: bool,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct ViewArgs {
    /// Release ID
    id: i64,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct DownloadArgs {
    /// Release ID
    id: i64,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct DeleteArgs {
    /// Release ID
    id: i64,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

impl ReleaseCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            ReleaseAction::List(args) => list_releases(args).await,
            ReleaseAction::Create(args) => create_release(args).await,
            ReleaseAction::View(args) => view_release(args).await,
            ReleaseAction::Download(args) => download_release(args).await,
            ReleaseAction::Delete(args) => delete_release(args).await,
        }
    }
}

async fn list_releases(args: &ListArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let releases = api
        .repo_list_releases()
        .owner(owner)
        .repo(repo)
        .page(1)
        .limit(30)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&releases)?);
        return Ok(());
    }

    if releases.is_empty() {
        eprintln!("No releases found");
        return Ok(());
    }

    let is_tty = atty_check();
    if is_tty {
        println!(
            "{:<6} {:<20} {:<30} {:<10} {}",
            "ID", "TAG", "TITLE", "STATUS", "PUBLISHED"
        );
    }

    for rel in &releases {
        let id = rel.id.unwrap_or(0);
        let tag = rel.tag_name.as_deref().unwrap_or("");
        let name = rel.name.as_deref().unwrap_or("");
        let truncated_name = if name.len() > 28 {
            format!("{}...", &name[..25])
        } else {
            name.to_string()
        };

        let status = if rel.draft.unwrap_or(false) {
            "draft"
        } else if rel.prerelease.unwrap_or(false) {
            "pre"
        } else {
            "latest"
        };

        let published = rel
            .published_at
            .map(|dt| relative_time(dt))
            .unwrap_or_default();

        println!(
            "{:<6} {:<20} {:<30} {:<10} {}",
            id, tag, truncated_name, status, published
        );
    }

    Ok(())
}

async fn create_release(args: &CreateArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let body_text = args.body.clone();
    let draft = args.draft;
    let prerelease = args.prerelease;

    let release = api
        .repo_create_release()
        .owner(owner)
        .repo(repo)
        .body_map(|mut b| {
            b = b
                .tag_name(args.tag.clone())
                .name(args.name.clone())
                .draft(draft)
                .prerelease(prerelease);
            if let Some(body) = body_text {
                b = b.body(body);
            }
            b
        })
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    let id = release.id.unwrap_or(0);
    let url = release.html_url.as_deref().unwrap_or("");
    eprintln!("Created release #{id}: {url}");
    Ok(())
}

async fn view_release(args: &ViewArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let rel = api
        .repo_get_release()
        .owner(owner)
        .repo(repo)
        .id(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&rel)?);
        return Ok(());
    }

    let name = rel.name.as_deref().unwrap_or("(no title)");
    let tag = rel.tag_name.as_deref().unwrap_or("");
    let id = rel.id.unwrap_or(0);

    let status = if rel.draft.unwrap_or(false) {
        "draft"
    } else if rel.prerelease.unwrap_or(false) {
        "prerelease"
    } else {
        "release"
    };

    println!("{name} (#{id})");
    println!("Tag: {tag} -- {status}");

    if let Some(ref author) = rel.author {
        let login = author.login.as_deref().unwrap_or("unknown");
        println!("Author: {login}");
    }

    if let Some(published) = rel.published_at {
        println!("Published: {}", relative_time(published));
    }

    // Body
    if let Some(ref body) = rel.body {
        if !body.is_empty() {
            println!();
            println!("{body}");
        }
    }

    // Assets
    if !rel.assets.is_empty() {
        println!("\nAssets:");
        for asset in &rel.assets {
            let name = asset.name.as_deref().unwrap_or("unnamed");
            let downloads = asset.download_count.unwrap_or(0);
            println!("  {name} ({downloads} downloads)");
        }
    }

    // URL
    if let Some(ref url) = rel.html_url {
        println!();
        println!("{url}");
    }

    Ok(())
}

async fn download_release(args: &DownloadArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let rel = api
        .repo_get_release()
        .owner(owner)
        .repo(repo)
        .id(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if rel.assets.is_empty() {
        eprintln!("No assets to download for release #{}", args.id);
        return Ok(());
    }

    for asset in &rel.assets {
        let url = asset
            .browser_download_url
            .as_deref()
            .ok_or_else(|| eyre::eyre!("Asset has no download URL"))?;
        let filename = asset.name.as_deref().unwrap_or("download");

        eprintln!("Downloading {filename}...");

        // Strip base URL prefix to get the API path for raw_request
        let path = url
            .strip_prefix(api.base_url())
            .map(|p| p.to_string())
            .unwrap_or_else(|| url.to_string());
        let resp = api
            .raw_request(gitea_api::Method::GET, &path, None)
            .await
            .map_err(|e| eyre::eyre!("Download failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            eyre::bail!(
                "Download failed: {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            );
        }

        let bytes = resp.bytes().await?;
        std::fs::write(filename, &bytes)?;
        eprintln!("  Saved {filename} ({} bytes)", bytes.len());
    }

    Ok(())
}

async fn delete_release(args: &DeleteArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    api.repo_delete_release()
        .owner(owner)
        .repo(repo)
        .id(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    eprintln!("Release #{} deleted", args.id);
    Ok(())
}
