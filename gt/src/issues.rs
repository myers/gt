use clap::{Args, Subcommand};
use eyre::Result;

use crate::config::Config;

#[derive(Args)]
pub struct IssueCommand {
    #[command(subcommand)]
    action: IssueAction,
}

#[derive(Subcommand)]
enum IssueAction {
    /// List issues
    List(ListArgs),
}

#[derive(Args)]
struct ListArgs {
    /// Repository (owner/repo). Required.
    #[arg(short = 'R', long)]
    repo: String,

    /// Filter by state
    #[arg(short, long, default_value = "open")]
    state: String,

    /// Maximum number of issues to show
    #[arg(short, long, default_value = "30")]
    limit: i64,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

impl IssueCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            IssueAction::List(args) => list_issues(args).await,
        }
    }
}

async fn list_issues(args: &ListArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let (owner, repo) = parse_repo(&args.repo)?;

    let mut req = api
        .issue_list_issues()
        .owner(owner)
        .repo(repo)
        .page(1)
        .limit(args.limit);

    match args.state.as_str() {
        "open" => req = req.state(gitea_api::types::IssueListIssuesState::Open),
        "closed" => req = req.state(gitea_api::types::IssueListIssuesState::Closed),
        "all" => {}
        other => eyre::bail!("Invalid state: {other}. Use open, closed, or all"),
    }

    let issues = req
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&issues)?);
        return Ok(());
    }

    if issues.is_empty() {
        eprintln!("No issues found");
        return Ok(());
    }

    // Table output
    let is_tty = atty_check();

    // Header
    if is_tty {
        println!(
            "{:<6} {:<50} {:<20} {}",
            "#", "TITLE", "LABELS", "UPDATED"
        );
    }

    for issue in &issues {
        let number = issue.number.unwrap_or(0);
        let title = issue.title.as_deref().unwrap_or("");
        let truncated_title = if title.len() > 48 {
            format!("{}...", &title[..45])
        } else {
            title.to_string()
        };

        let labels = issue
            .labels
            .iter()
            .filter_map(|l| l.name.as_deref())
            .collect::<Vec<_>>()
            .join(",");
        let truncated_labels = if labels.len() > 18 {
            format!("{}...", &labels[..15])
        } else {
            labels
        };

        let updated = issue
            .updated_at
            .map(|dt| relative_time(dt))
            .unwrap_or_default();

        println!(
            "{:<6} {:<50} {:<20} {}",
            number, truncated_title, truncated_labels, updated
        );
    }

    Ok(())
}

fn parse_repo(s: &str) -> Result<(&str, &str)> {
    let parts: Vec<&str> = s.splitn(2, '/').collect();
    if parts.len() != 2 {
        eyre::bail!("Repository must be in owner/repo format, got: {s}");
    }
    Ok((parts[0], parts[1]))
}

fn relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let delta = now.signed_duration_since(dt);

    if delta.num_minutes() < 1 {
        "just now".to_string()
    } else if delta.num_hours() < 1 {
        format!("{}m ago", delta.num_minutes())
    } else if delta.num_days() < 1 {
        format!("{}h ago", delta.num_hours())
    } else if delta.num_weeks() < 1 {
        format!("{}d ago", delta.num_days())
    } else if delta.num_weeks() < 52 {
        format!("{}w ago", delta.num_weeks())
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

fn atty_check() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}
