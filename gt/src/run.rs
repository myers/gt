use clap::{Args, Subcommand};
use eyre::Result;

use crate::config::Config;
use crate::issues::{atty_check, relative_time};
use crate::repo;

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "success" | "failure" | "cancelled" | "skipped" | "timed_out" | "action_required",
    )
}

fn is_failure_conclusion(conclusion: &str) -> bool {
    matches!(
        conclusion,
        "failure" | "cancelled" | "timed_out" | "action_required",
    )
}

fn job_icon(status: &str, conclusion: &str) -> &'static str {
    match (status, conclusion) {
        (_, "success") => "✓",
        (_, "failure") => "✗",
        (_, "cancelled") => "⊘",
        (_, "timed_out") => "✗",
        (_, "action_required") => "✗",
        ("in_progress", _) => "●",
        ("queued", _) | ("waiting", _) => "○",
        _ => "?",
    }
}

fn render_run_state(
    run: &gitea_api::types::ActionWorkflowRun,
    jobs: &[gitea_api::types::ActionWorkflowJob],
    compact: bool,
) -> String {
    let _ = compact; // wired in Task 4
    let mut out = String::new();
    let id = run.id.unwrap_or(0);
    let title = run.display_title.as_deref().unwrap_or("(unnamed)");
    let status = run.status.as_deref().unwrap_or("unknown");
    let conclusion = run.conclusion.as_deref().unwrap_or("");

    out.push_str(&format!("Run #{id} — {title}\n"));
    if conclusion.is_empty() {
        out.push_str(&format!("Status: {status}\n"));
    } else {
        out.push_str(&format!("Status: {status} ({conclusion})\n"));
    }
    out.push('\n');

    for job in jobs {
        let jname = job.name.as_deref().unwrap_or("(unnamed job)");
        let jstatus = job.status.as_deref().unwrap_or("");
        let jconclusion = job.conclusion.as_deref().unwrap_or("");
        let icon = job_icon(jstatus, jconclusion);
        if jconclusion.is_empty() {
            out.push_str(&format!("  {icon} {jname} ({jstatus})\n"));
        } else if jconclusion == "success" {
            out.push_str(&format!("  {icon} {jname}\n"));
        } else {
            out.push_str(&format!("  {icon} {jname} ({jconclusion})\n"));
        }
        for step in &job.steps {
            let sname = step.name.as_deref().unwrap_or("(unnamed step)");
            let sstatus = step.status.as_deref().unwrap_or("");
            let sconclusion = step.conclusion.as_deref().unwrap_or("");
            let sicon = job_icon(sstatus, sconclusion);
            out.push_str(&format!("    {sicon} {sname}\n"));
        }
    }

    out
}

#[derive(Args)]
pub struct RunCommand {
    #[command(flatten)]
    pub repo: repo::RepoArgs,

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
    /// Watch a workflow run (poll until complete, show logs)
    Watch(WatchArgs),
    /// Download artifacts from a workflow run
    Download(DownloadArgs),
}

#[derive(Args)]
struct ListArgs {
    #[command(flatten)]
    json: crate::json::JsonArgs,
}

#[derive(Args)]
struct ViewArgs {
    id: i64,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct RerunArgs {
    id: i64,
}

#[derive(Args)]
struct WatchArgs {
    /// Run ID. If omitted, prompt to pick from in-progress runs.
    id: Option<i64>,

    /// Refresh interval in seconds.
    #[arg(short = 'i', long, default_value = "3")]
    interval: u64,

    /// Hide successful steps; show only relevant/failed steps.
    #[arg(long)]
    compact: bool,

    /// Exit non-zero if the run's conclusion is a failure. By default,
    /// `gt run watch` exits 0 once the run reaches a terminal state,
    /// regardless of conclusion (matches `gh run watch`).
    #[arg(long = "exit-status")]
    exit_status: bool,
}

#[derive(Args)]
struct DownloadArgs {
    /// Run ID
    id: i64,

    /// Output directory (defaults to current directory)
    #[arg(short, long, default_value = ".")]
    dir: String,
}

impl RunCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            RunAction::List(args) => list_runs(&self.repo, args).await,
            RunAction::View(args) => view_run(&self.repo, args).await,
            RunAction::Rerun(args) => rerun_run(&self.repo, args).await,
            RunAction::Watch(args) => watch_run(&self.repo, args).await,
            RunAction::Download(args) => download_artifacts(&self.repo, args).await,
        }
    }
}

const RUN_FIELDS: &[&str] = &[
    "id", "display_title", "status", "conclusion", "event", "head_branch",
    "head_sha", "html_url", "started_at", "completed_at", "created_at",
    "updated_at",
];

async fn list_runs(repo_args: &repo::RepoArgs, args: &ListArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;
    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;

    let resp = api
        .get_workflow_runs()
        .owner(&repo_info.owner)
        .repo(&repo_info.name)
        .page(1)
        .limit(30)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
        .into_inner();

    if args.json.is_json() {
        return crate::json::write_json(&args.json, &resp.workflow_runs, &RUN_FIELDS);
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

async fn view_run(repo_args: &repo::RepoArgs, args: &ViewArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;
    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;

    let run = api
        .get_workflow_run()
        .owner(&repo_info.owner)
        .repo(&repo_info.name)
        .run(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
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

async fn rerun_run(repo_args: &repo::RepoArgs, args: &RerunArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;
    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;

    api.rerun_workflow_run()
        .owner(&repo_info.owner)
        .repo(&repo_info.name)
        .run(args.id)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?;

    eprintln!("Rerun triggered for run #{}", args.id);
    Ok(())
}

async fn watch_run(repo_args: &repo::RepoArgs, args: &WatchArgs) -> Result<()> {
    let _ = (repo_args, args);
    eyre::bail!("watch_run: not yet implemented")
}

async fn download_artifacts(repo_args: &repo::RepoArgs, args: &DownloadArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;
    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    // List artifacts for this run
    let resp = api
        .raw_get(&format!("repos/{owner}/{repo}/actions/runs/{}/artifacts", args.id))
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;
    let data: serde_json::Value = serde_json::from_str(&resp)?;

    let artifacts = data
        .get("artifacts")
        .and_then(|a| a.as_array())
        .ok_or_else(|| eyre::eyre!("No artifacts found for run #{}", args.id))?;

    if artifacts.is_empty() {
        eprintln!("No artifacts found for run #{}", args.id);
        return Ok(());
    }

    let out_dir = std::path::Path::new(&args.dir);
    std::fs::create_dir_all(out_dir)?;

    for artifact in artifacts {
        let name = artifact["name"].as_str().unwrap_or("artifact");
        let artifact_id = artifact["id"]
            .as_i64()
            .ok_or_else(|| eyre::eyre!("Artifact missing ID"))?;

        let download_path = format!(
            "repos/{owner}/{repo}/actions/artifacts/{artifact_id}"
        );
        let resp = api
            .raw_request(gitea_api::Method::GET, &download_path, None)
            .await
            .map_err(|e| eyre::eyre!("{e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            eyre::bail!("Failed to download {name}: HTTP {}", status.as_u16());
        }

        let bytes = resp.bytes().await?;
        let filename = format!("{name}.zip");
        let dest = out_dir.join(&filename);
        std::fs::write(&dest, &bytes)?;
        eprintln!("Downloaded {filename} ({} bytes)", bytes.len());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_status_recognises_completed_and_outcome_aliases() {
        assert!(is_terminal_status("completed"));
        assert!(is_terminal_status("success"));
        assert!(is_terminal_status("failure"));
        assert!(is_terminal_status("cancelled"));
        assert!(is_terminal_status("skipped"));
        assert!(is_terminal_status("timed_out"));
        assert!(is_terminal_status("action_required"));
    }

    #[test]
    fn terminal_status_rejects_in_flight_states() {
        assert!(!is_terminal_status("in_progress"));
        assert!(!is_terminal_status("queued"));
        assert!(!is_terminal_status("waiting"));
        assert!(!is_terminal_status(""));
    }

    #[test]
    fn failure_conclusion_truth_table() {
        assert!(is_failure_conclusion("failure"));
        assert!(is_failure_conclusion("cancelled"));
        assert!(is_failure_conclusion("timed_out"));
        assert!(is_failure_conclusion("action_required"));

        assert!(!is_failure_conclusion("success"));
        assert!(!is_failure_conclusion("skipped"));
        assert!(!is_failure_conclusion(""));
        assert!(!is_failure_conclusion("in_progress"));
    }

    fn make_run(id: i64, title: &str, status: &str) -> gitea_api::types::ActionWorkflowRun {
        gitea_api::types::ActionWorkflowRun {
            id: Some(id),
            display_title: Some(title.to_string()),
            status: Some(status.to_string()),
            ..Default::default()
        }
    }

    fn make_step(name: &str, status: &str, conclusion: Option<&str>) -> gitea_api::types::ActionWorkflowStep {
        gitea_api::types::ActionWorkflowStep {
            name: Some(name.to_string()),
            status: Some(status.to_string()),
            conclusion: conclusion.map(str::to_string),
            ..Default::default()
        }
    }

    fn make_job(
        name: &str,
        status: &str,
        conclusion: Option<&str>,
        steps: Vec<gitea_api::types::ActionWorkflowStep>,
    ) -> gitea_api::types::ActionWorkflowJob {
        gitea_api::types::ActionWorkflowJob {
            name: Some(name.to_string()),
            status: Some(status.to_string()),
            conclusion: conclusion.map(str::to_string),
            steps,
            ..Default::default()
        }
    }

    #[test]
    fn render_default_shows_every_step() {
        let run = make_run(42, "feat: hello", "in_progress");
        let jobs = vec![
            make_job(
                "build",
                "completed",
                Some("success"),
                vec![
                    make_step("checkout", "completed", Some("success")),
                    make_step("cargo test", "completed", Some("success")),
                ],
            ),
            make_job(
                "lint",
                "in_progress",
                None,
                vec![
                    make_step("checkout", "completed", Some("success")),
                    make_step("clippy", "in_progress", None),
                ],
            ),
        ];

        let out = render_run_state(&run, &jobs, false);

        assert!(out.contains("Run #42 — feat: hello"), "header missing: {out}");
        assert!(out.contains("Status: in_progress"), "status missing: {out}");
        assert!(out.contains("✓ build"));
        assert!(out.contains("✓ checkout"));
        assert!(out.contains("✓ cargo test"));
        assert!(out.contains("● lint"));
        assert!(out.contains("● clippy"));
    }
}
