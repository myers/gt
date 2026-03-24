use clap::{Args, Subcommand};
use eyre::Result;

use crate::config::Config;
use crate::issues::{atty_check, relative_time};
use crate::repo;

#[derive(Args)]
pub struct RunCommand {
    #[command(subcommand)]
    action: RunAction,
}

#[derive(Subcommand)]
enum RunAction {
    /// List workflow runs
    List(ListArgs),
    /// View a workflow run
    View(ViewArgs),
    /// Rerun a workflow run
    Rerun(RerunArgs),
}

#[derive(Args)]
struct ListArgs {
    #[arg(short = 'R', long)]
    repo: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ViewArgs {
    id: i64,
    #[arg(short = 'R', long)]
    repo: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct RerunArgs {
    id: i64,
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

impl RunCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            RunAction::List(args) => list_runs(args).await,
            RunAction::View(args) => view_run(args).await,
            RunAction::Rerun(args) => rerun_run(args).await,
        }
    }
}

async fn list_runs(args: &ListArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;
    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;

    let resp = api
        .get_workflow_runs()
        .owner(&repo_info.owner)
        .repo(&repo_info.name)
        .page(1)
        .limit(30)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }

    let runs = &resp.workflow_runs;

    if runs.is_empty() {
        eprintln!("No workflow runs found");
        return Ok(());
    }

    let is_tty = atty_check();
    if is_tty {
        println!(
            "{:<8} {:<30} {:<12} {:<10} {}",
            "ID", "TITLE", "STATUS", "BRANCH", "STARTED"
        );
    }

    for run in runs {
        let id = run.id.unwrap_or(0);
        let title = run.display_title.as_deref().unwrap_or("");
        let truncated = if title.len() > 28 {
            format!("{}...", &title[..25])
        } else {
            title.to_string()
        };
        let status = run.status.as_deref().unwrap_or("");
        let branch = run.head_branch.as_deref().unwrap_or("");
        let started = run
            .started_at
            .map(|dt| relative_time(dt))
            .unwrap_or_default();

        println!("{:<8} {:<30} {:<12} {:<10} {}", id, truncated, status, branch, started);
    }

    Ok(())
}

async fn view_run(args: &ViewArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;
    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;

    let run = api
        .get_workflow_run()
        .owner(&repo_info.owner)
        .repo(&repo_info.name)
        .run(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&run)?);
        return Ok(());
    }

    let title = run.display_title.as_deref().unwrap_or("(unnamed)");
    let id = run.id.unwrap_or(0);
    let status = run.status.as_deref().unwrap_or("unknown");
    let conclusion = run.conclusion.as_deref().unwrap_or("");
    let branch = run.head_branch.as_deref().unwrap_or("");
    let event = run.event.as_deref().unwrap_or("");

    println!("{title} (#{id})");
    println!("Status: {status}{}", if conclusion.is_empty() { String::new() } else { format!(" ({conclusion})") });
    println!("Branch: {branch}");
    println!("Event: {event}");

    if let Some(ref url) = run.html_url {
        println!();
        println!("{url}");
    }

    Ok(())
}

async fn rerun_run(args: &RerunArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;
    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;

    api.rerun_workflow_run()
        .owner(&repo_info.owner)
        .repo(&repo_info.name)
        .run(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    eprintln!("Rerun triggered for run #{}", args.id);
    Ok(())
}
