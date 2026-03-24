use clap::{Args, Subcommand};
use eyre::Result;

use crate::config::Config;
use crate::issues::{atty_check, relative_time};
use crate::paginate;
use crate::repo;

#[derive(Args)]
pub struct ReleaseCommand {
    #[command(flatten)]
    pub repo: repo::RepoArgs,

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
    #[command(flatten)]
    json: crate::json::JsonArgs,
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
}

#[derive(Args)]
struct ViewArgs {
    /// Release ID
    id: i64,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct DownloadArgs {
    /// Release ID
    id: i64,
}

#[derive(Args)]
struct DeleteArgs {
    /// Release ID
    id: i64,
}

impl ReleaseCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            ReleaseAction::List(args) => list_releases(&self.repo, args).await,
            ReleaseAction::Create(args) => create_release(&self.repo, args).await,
            ReleaseAction::View(args) => view_release(&self.repo, args).await,
            ReleaseAction::Download(args) => download_release(&self.repo, args).await,
            ReleaseAction::Delete(args) => delete_release(&self.repo, args).await,
        }
    }
}

const RELEASE_FIELDS: &[&str] = &[
    "id", "tag_name", "name", "body", "draft", "prerelease",
    "created_at", "published_at", "url", "html_url", "tarball_url", "zipball_url",
    "assets",
];

async fn list_releases(repo_args: &repo::RepoArgs, args: &ListArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let releases = paginate::paginate(200, 50, |page, per_page| {
        let api = &api;
        async move {
            Ok(api
                .repo_list_releases()
                .owner(owner)
                .repo(repo)
                .page(page)
                .limit(per_page)
                .send()
                .await
                .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
                .into_inner())
        }
    })
    .await?;

    if args.json.is_json() {
        return crate::json::write_json(&args.json, &releases, &RELEASE_FIELDS);
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

async fn create_release(repo_args: &repo::RepoArgs, args: &CreateArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;
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
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
        .into_inner();

    let id = release.id.unwrap_or(0);
    let url = release.html_url.as_deref().unwrap_or("");
    eprintln!("Created release #{id}: {url}");
    Ok(())
}

async fn view_release(repo_args: &repo::RepoArgs, args: &ViewArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let rel = api
        .repo_get_release()
        .owner(owner)
        .repo(repo)
        .id(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
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

async fn download_release(repo_args: &repo::RepoArgs, args: &DownloadArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let rel = api
        .repo_get_release()
        .owner(owner)
        .repo(repo)
        .id(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
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

async fn delete_release(repo_args: &repo::RepoArgs, args: &DeleteArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    api.repo_delete_release()
        .owner(owner)
        .repo(repo)
        .id(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?;

    eprintln!("Release #{} deleted", args.id);
    Ok(())
}
