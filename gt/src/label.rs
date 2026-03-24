use clap::{Args, Subcommand};
use eyre::Result;

use crate::config::Config;
use crate::issues::atty_check;
use crate::repo;

#[derive(Args)]
pub struct LabelCommand {
    #[command(subcommand)]
    action: LabelAction,
}

#[derive(Subcommand)]
enum LabelAction {
    /// List labels
    List(ListArgs),
    /// Create a label
    Create(CreateArgs),
    /// Edit a label
    Edit(EditArgs),
    /// Delete a label
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
    /// Label name
    #[arg(short, long)]
    name: String,

    /// Label color (hex, e.g. "ff0000")
    #[arg(short, long)]
    color: String,

    /// Label description
    #[arg(short, long)]
    description: Option<String>,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct EditArgs {
    /// Label ID
    id: i64,

    /// New name
    #[arg(short, long)]
    name: Option<String>,

    /// New color (hex)
    #[arg(short, long)]
    color: Option<String>,

    /// New description
    #[arg(short, long)]
    description: Option<String>,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct DeleteArgs {
    /// Label ID
    id: i64,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

impl LabelCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            LabelAction::List(args) => list_labels(args).await,
            LabelAction::Create(args) => create_label(args).await,
            LabelAction::Edit(args) => edit_label(args).await,
            LabelAction::Delete(args) => delete_label(args).await,
        }
    }
}

async fn list_labels(args: &ListArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let labels = api
        .issue_list_labels()
        .owner(owner)
        .repo(repo)
        .page(1)
        .limit(50)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&labels)?);
        return Ok(());
    }

    if labels.is_empty() {
        eprintln!("No labels found");
        return Ok(());
    }

    let is_tty = atty_check();
    if is_tty {
        println!("{:<6} {:<30} {:<10} {}", "ID", "NAME", "COLOR", "DESCRIPTION");
    }

    for label in &labels {
        let id = label.id.unwrap_or(0);
        let name = label.name.as_deref().unwrap_or("");
        let color = label.color.as_deref().unwrap_or("");
        let desc = label.description.as_deref().unwrap_or("");
        println!("{:<6} {:<30} {:<10} {}", id, name, color, desc);
    }

    Ok(())
}

async fn create_label(args: &CreateArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let description = args.description.clone();
    let label = api
        .issue_create_label()
        .owner(owner)
        .repo(repo)
        .body_map(|mut b| {
            b = b.name(args.name.clone()).color(args.color.clone());
            if let Some(desc) = description {
                b = b.description(desc);
            }
            b
        })
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    let id = label.id.unwrap_or(0);
    let name = label.name.as_deref().unwrap_or("");
    eprintln!("Created label #{id}: {name}");
    Ok(())
}

async fn edit_label(args: &EditArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let name = args.name.clone();
    let color = args.color.clone();
    let description = args.description.clone();

    api.issue_edit_label()
        .owner(owner)
        .repo(repo)
        .id(args.id)
        .body_map(move |mut b| {
            if let Some(n) = name {
                b = b.name(n);
            }
            if let Some(c) = color {
                b = b.color(c);
            }
            if let Some(d) = description {
                b = b.description(d);
            }
            b
        })
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    eprintln!("Label #{} updated", args.id);
    Ok(())
}

async fn delete_label(args: &DeleteArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    api.issue_delete_label()
        .owner(owner)
        .repo(repo)
        .id(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    eprintln!("Label #{} deleted", args.id);
    Ok(())
}
