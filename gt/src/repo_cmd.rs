use clap::{Args, Subcommand};
use eyre::Result;

use crate::config::Config;
use crate::issues::{atty_check, relative_time};
use crate::paginate;
use crate::repo;

#[derive(Args)]
pub struct RepoCommand {
    #[command(flatten)]
    pub repo: repo::RepoArgs,

    #[command(subcommand)]
    action: RepoAction,
}

#[derive(Subcommand)]
enum RepoAction {
    /// View repository info
    View(ViewArgs),
    /// List repositories for a user or organization
    List(ListArgs),
    /// Clone a repository
    Clone(CloneArgs),
    /// Create a new repository
    Create(CreateArgs),
    /// Fork a repository
    Fork(ForkArgs),
}

#[derive(Args)]
struct ViewArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ListArgs {
    /// User or organization name (defaults to authenticated user)
    owner: Option<String>,

    /// Maximum number of repos to show
    #[arg(short, long, default_value = "30")]
    limit: i64,

    #[command(flatten)]
    json: crate::json::JsonArgs,
}

#[derive(Args)]
struct CloneArgs {
    /// Repository to clone (owner/repo)
    repo: String,
}

#[derive(Args)]
struct CreateArgs {
    /// Repository name
    name: String,

    /// Repository description
    #[arg(short, long)]
    description: Option<String>,

    /// Make repository private
    #[arg(long)]
    private: bool,
}

#[derive(Args)]
struct ForkArgs {
    /// Repository to fork (owner/repo)
    repo: String,
}

impl RepoCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            RepoAction::View(args) => view_repo(&self.repo, args).await,
            RepoAction::List(args) => list_repos(args).await,
            RepoAction::Clone(args) => clone_repo(args).await,
            RepoAction::Create(args) => create_repo(args).await,
            RepoAction::Fork(args) => fork_repo(args).await,
        }
    }
}

async fn view_repo(repo_args: &repo::RepoArgs, args: &ViewArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;

    let repo_data = api
        .repo_get()
        .owner(&repo_info.owner)
        .repo(&repo_info.name)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
        .into_inner();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&repo_data)?);
        return Ok(());
    }

    let name = repo_data.full_name.as_deref().unwrap_or("");
    let desc = repo_data.description.as_deref().unwrap_or("No description");
    let default_branch = repo_data.default_branch.as_deref().unwrap_or("main");
    let stars = repo_data.stars_count.unwrap_or(0);
    let forks = repo_data.forks_count.unwrap_or(0);
    let open_issues = repo_data.open_issues_count.unwrap_or(0);
    let private = repo_data.private.unwrap_or(false);

    println!("{name}");
    println!("{desc}");
    println!();
    println!(
        "{} — {} stars — {} forks — {} open issues — default: {default_branch}",
        if private { "Private" } else { "Public" },
        stars,
        forks,
        open_issues,
    );

    if let Some(ref url) = repo_data.html_url {
        println!("{url}");
    }

    Ok(())
}

const REPO_FIELDS: &[&str] = &[
    "id", "name", "full_name", "description", "private", "fork", "archived",
    "stars_count", "forks_count", "open_issues_count", "default_branch",
    "created_at", "updated_at", "html_url", "clone_url", "ssh_url",
];

async fn list_repos(args: &ListArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repos = if let Some(ref owner) = args.owner {
        let owner = owner.as_str();
        paginate::paginate(args.limit, 50, |page, per_page| {
            let api = &api;
            async move {
                Ok(api
                    .user_list_repos()
                    .username(owner)
                    .page(page)
                    .limit(per_page)
                    .send()
                    .await
                    .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
                    .into_inner())
            }
        })
        .await?
    } else {
        paginate::paginate(args.limit, 50, |page, per_page| {
            let api = &api;
            async move {
                Ok(api
                    .user_current_list_repos()
                    .page(page)
                    .limit(per_page)
                    .send()
                    .await
                    .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
                    .into_inner())
            }
        })
        .await?
    };

    if args.json.is_json() {
        return crate::json::write_json(&args.json, &repos, &REPO_FIELDS);
    }

    if repos.is_empty() {
        eprintln!("No repositories found");
        return Ok(());
    }

    let is_tty = atty_check();
    if is_tty {
        println!("{:<40} {:<10} {}", "REPO", "STARS", "UPDATED");
    }

    for r in &repos {
        let name = r.full_name.as_deref().unwrap_or("");
        let stars = r.stars_count.unwrap_or(0);
        let updated = r
            .updated_at
            .map(|dt| relative_time(dt))
            .unwrap_or_default();
        println!("{:<40} {:<10} {}", name, stars, updated);
    }

    Ok(())
}

async fn clone_repo(args: &CloneArgs) -> Result<()> {
    let config = Config::load()?;

    let clone_url = format!(
        "{}{}.git",
        config.url.as_str().trim_end_matches('/'),
        if args.repo.starts_with('/') {
            args.repo.clone()
        } else {
            format!("/{}", args.repo)
        },
    );

    let status = std::process::Command::new("git")
        .args(["clone", &clone_url])
        .status()
        .map_err(|e| eyre::eyre!("git clone failed: {e}"))?;

    if !status.success() {
        eyre::bail!("git clone failed");
    }

    Ok(())
}

async fn create_repo(args: &CreateArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let description = args.description.clone();
    let private = args.private;

    let repo_data = api
        .create_current_user_repo()
        .body_map(|mut b| {
            b = b.name(args.name.clone()).private(private).auto_init(true);
            if let Some(desc) = description {
                b = b.description(desc);
            }
            b
        })
        .send()
        .await
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
        .into_inner();

    let full_name = repo_data.full_name.as_deref().unwrap_or("");
    let url = repo_data.html_url.as_deref().unwrap_or("");
    eprintln!("Created repository {full_name}: {url}");
    Ok(())
}

async fn fork_repo(args: &ForkArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::parse_repo(&args.repo)?;

    let forked = api
        .create_fork()
        .owner(&repo_info.owner)
        .repo(&repo_info.name)
        .body_map(|b| b)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
        .into_inner();

    let full_name = forked.full_name.as_deref().unwrap_or("");
    let url = forked.html_url.as_deref().unwrap_or("");
    eprintln!("Forked to {full_name}: {url}");
    Ok(())
}
