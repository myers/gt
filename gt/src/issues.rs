use clap::{Args, Subcommand};
use eyre::Result;

use crate::config::Config;
use crate::repo;

#[derive(Args)]
pub struct IssueCommand {
    #[command(subcommand)]
    action: IssueAction,
}

#[derive(Subcommand)]
enum IssueAction {
    /// List issues
    List(ListArgs),
    /// View an issue
    View(ViewArgs),
    /// Create an issue
    Create(CreateArgs),
    /// Close an issue
    Close(CloseArgs),
    /// Reopen an issue
    Reopen(ReopenArgs),
    /// Add a comment to an issue
    Comment(CommentArgs),
    /// Edit an issue
    Edit(EditArgs),
    /// Show status of relevant issues
    Status(StatusArgs),
}

#[derive(Args)]
struct EditArgs {
    /// Issue number
    number: i64,

    /// New title
    #[arg(short, long)]
    title: Option<String>,

    /// New body
    #[arg(short, long)]
    body: Option<String>,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct StatusArgs {
    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct ListArgs {
    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,

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

#[derive(Args)]
struct ViewArgs {
    /// Issue number
    number: i64,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,

    /// Show comments
    #[arg(short, long)]
    comments: bool,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct CreateArgs {
    /// Issue title
    #[arg(short, long)]
    title: String,

    /// Issue body
    #[arg(short, long, default_value = "")]
    body: String,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,

    /// Labels (comma-separated names — looked up by name)
    #[arg(short, long)]
    label: Vec<String>,

    /// Assignees (comma-separated usernames)
    #[arg(short, long)]
    assignee: Vec<String>,
}

#[derive(Args)]
struct CloseArgs {
    /// Issue number
    number: i64,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct ReopenArgs {
    /// Issue number
    number: i64,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct CommentArgs {
    /// Issue number
    number: i64,

    /// Comment body
    #[arg(short, long)]
    body: String,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

impl IssueCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            IssueAction::List(args) => list_issues(args).await,
            IssueAction::View(args) => view_issue(args).await,
            IssueAction::Create(args) => create_issue(args).await,
            IssueAction::Close(args) => set_issue_state(args.repo.as_deref(), args.number, "closed").await,
            IssueAction::Reopen(args) => set_issue_state(args.repo.as_deref(), args.number, "open").await,
            IssueAction::Comment(args) => comment_issue(args).await,
            IssueAction::Edit(args) => edit_issue(args).await,
            IssueAction::Status(args) => status_issues(args).await,
        }
    }
}

async fn list_issues(args: &ListArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

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

async fn view_issue(args: &ViewArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let issue = api
        .issue_get_issue()
        .owner(owner)
        .repo(repo)
        .index(args.number)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if args.json {
        if args.comments {
            let comments = api
                .issue_get_comments()
                .owner(owner)
                .repo(repo)
                .index(args.number)
                .send()
                .await
                .map_err(|e| eyre::eyre!("{e}"))?
                .into_inner();
            let combined = serde_json::json!({
                "issue": issue,
                "comments": comments,
            });
            println!("{}", serde_json::to_string_pretty(&combined)?);
        } else {
            println!("{}", serde_json::to_string_pretty(&issue)?);
        }
        return Ok(());
    }

    // Header: title and number
    let number = issue.number.unwrap_or(0);
    let title = issue.title.as_deref().unwrap_or("(no title)");
    let state = issue
        .state
        .as_ref()
        .map(|s| format!("{s:?}"))
        .unwrap_or_default()
        .to_lowercase();

    println!("{title} #{number}");
    println!("{state} — opened by {} {}",
        issue.user.as_ref().and_then(|u| u.login.as_deref()).unwrap_or("unknown"),
        issue.created_at.map(|dt| relative_time(dt)).unwrap_or_default(),
    );

    // Labels
    if !issue.labels.is_empty() {
        let label_names: Vec<&str> = issue.labels.iter().filter_map(|l| l.name.as_deref()).collect();
        println!("Labels: {}", label_names.join(", "));
    }

    // Assignees
    if !issue.assignees.is_empty() {
        let names: Vec<&str> = issue.assignees.iter().filter_map(|u| u.login.as_deref()).collect();
        println!("Assignees: {}", names.join(", "));
    }

    // Milestone
    if let Some(ref ms) = issue.milestone {
        if let Some(ref title) = ms.title {
            println!("Milestone: {title}");
        }
    }

    // Body
    if let Some(ref body) = issue.body {
        if !body.is_empty() {
            println!();
            println!("{body}");
        }
    }

    // URL
    if let Some(ref url) = issue.html_url {
        println!();
        println!("{url}");
    }

    // Comments
    if args.comments {
        let comments = api
            .issue_get_comments()
            .owner(owner)
            .repo(repo)
            .index(args.number)
            .send()
            .await
            .map_err(|e| eyre::eyre!("{e}"))?
            .into_inner();

        if comments.is_empty() {
            println!("\nNo comments.");
        } else {
            println!("\n--- {} comment{} ---", comments.len(), if comments.len() == 1 { "" } else { "s" });
            for c in &comments {
                let author = c.user.as_ref().and_then(|u| u.login.as_deref()).unwrap_or("unknown");
                let when = c.created_at.map(|dt| relative_time(dt)).unwrap_or_default();
                let body = c.body.as_deref().unwrap_or("");
                println!("\n{author} ({when}):");
                println!("{body}");
            }
        }
    } else {
        let count = issue.comments.unwrap_or(0);
        if count > 0 {
            println!("\n{count} comment{} (use -c to show)", if count == 1 { "" } else { "s" });
        }
    }

    Ok(())
}

async fn create_issue(args: &CreateArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let issue = api
        .issue_create_issue()
        .owner(owner)
        .repo(repo)
        .body_map(|b| {
            b.title(args.title.clone())
                .body(args.body.clone())
                .assignees(args.assignee.clone())
        })
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    let number = issue.number.unwrap_or(0);
    let url = issue.html_url.as_deref().unwrap_or("");
    eprintln!("Created issue #{number}: {url}");

    Ok(())
}

async fn set_issue_state(repo_opt: Option<&str>, number: i64, state: &str) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(repo_opt, &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    api.issue_edit_issue()
        .owner(owner)
        .repo(repo)
        .index(number)
        .body_map(|b| b.state(state.to_string()))
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    eprintln!("Issue #{number} {state}");
    Ok(())
}

async fn comment_issue(args: &CommentArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    api.issue_create_comment()
        .owner(owner)
        .repo(repo)
        .index(args.number)
        .body_map(|b| b.body(args.body.clone()))
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    eprintln!("Comment added to issue #{}", args.number);
    Ok(())
}

async fn edit_issue(args: &EditArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let mut builder = api.issue_edit_issue().owner(owner).repo(repo).index(args.number);

    if args.title.is_some() || args.body.is_some() {
        let title = args.title.clone();
        let body = args.body.clone();
        builder = builder.body_map(move |mut b| {
            if let Some(t) = title {
                b = b.title(t);
            }
            if let Some(bd) = body {
                b = b.body(bd);
            }
            b
        });
    }

    builder
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    eprintln!("Issue #{} updated", args.number);
    Ok(())
}

async fn status_issues(args: &StatusArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    // Show open issues assigned to the current user
    let issues = api
        .issue_list_issues()
        .owner(owner)
        .repo(repo)
        .state(gitea_api::types::IssueListIssuesState::Open)
        .page(1)
        .limit(20)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if issues.is_empty() {
        println!("No open issues");
        return Ok(());
    }

    println!("Open issues in {}/{}:", owner, repo);
    for issue in &issues {
        let number = issue.number.unwrap_or(0);
        let title = issue.title.as_deref().unwrap_or("");
        let updated = issue
            .updated_at
            .map(|dt| relative_time(dt))
            .unwrap_or_default();
        println!("  #{number:<5} {title} ({updated})");
    }

    Ok(())
}

pub fn relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
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

pub fn atty_check() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_relative_time_just_now() {
        let now = Utc::now();
        assert_eq!(relative_time(now), "just now");
    }

    #[test]
    fn test_relative_time_minutes() {
        let t = Utc::now() - Duration::minutes(5);
        assert_eq!(relative_time(t), "5m ago");
    }

    #[test]
    fn test_relative_time_hours() {
        let t = Utc::now() - Duration::hours(3);
        assert_eq!(relative_time(t), "3h ago");
    }

    #[test]
    fn test_relative_time_days() {
        let t = Utc::now() - Duration::days(4);
        assert_eq!(relative_time(t), "4d ago");
    }

    #[test]
    fn test_relative_time_weeks() {
        let t = Utc::now() - Duration::weeks(10);
        assert_eq!(relative_time(t), "10w ago");
    }

    #[test]
    fn test_relative_time_old_date() {
        let t = Utc::now() - Duration::weeks(60);
        assert!(relative_time(t).contains('-')); // returns YYYY-MM-DD format
    }
}
