use clap::{Args, Subcommand};
use eyre::Result;

use crate::config::Config;
use crate::issues::{atty_check, relative_time};
use crate::repo;

#[derive(Args)]
pub struct PrCommand {
    #[command(subcommand)]
    action: PrAction,
}

#[derive(Subcommand)]
enum PrAction {
    /// List pull requests
    List(ListArgs),
    /// View a pull request
    View(ViewArgs),
    /// Create a pull request
    Create(CreateArgs),
    /// Checkout a pull request branch
    Checkout(CheckoutArgs),
    /// Merge a pull request
    Merge(MergeArgs),
    /// Close a pull request
    Close(CloseArgs),
    /// Reopen a pull request
    Reopen(ReopenArgs),
    /// Add a comment to a pull request
    Comment(CommentArgs),
    /// View pull request diff
    Diff(DiffArgs),
    /// Submit a review on a pull request
    Review(ReviewArgs),
    /// Show CI status for a pull request
    Checks(ChecksArgs),
}

#[derive(Args)]
struct ListArgs {
    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,

    /// Filter by state (open, closed, all)
    #[arg(short, long, default_value = "open")]
    state: String,

    /// Maximum number of PRs to show
    #[arg(short, long, default_value = "30")]
    limit: i64,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ViewArgs {
    /// Pull request number
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
    /// PR title
    #[arg(short, long)]
    title: String,

    /// PR body
    #[arg(short, long, default_value = "")]
    body: String,

    /// Base branch (defaults to repo default branch)
    #[arg(long, default_value = "main")]
    base: String,

    /// Head branch (defaults to current branch)
    #[arg(long)]
    head: Option<String>,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct CheckoutArgs {
    /// PR number
    number: i64,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct MergeArgs {
    /// PR number
    number: i64,

    /// Merge method (merge, rebase, squash)
    #[arg(short, long, default_value = "merge")]
    method: String,

    /// Delete branch after merge
    #[arg(short, long)]
    delete_branch: bool,

    /// Repository (owner/repo). Detected from git remote if omitted.
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct CloseArgs {
    /// PR number
    number: i64,
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct ReopenArgs {
    /// PR number
    number: i64,
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct CommentArgs {
    /// PR number
    number: i64,

    /// Comment body
    #[arg(short, long)]
    body: String,

    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct ReviewArgs {
    /// PR number
    number: i64,

    /// Review action: approve, request-changes, comment
    #[arg(short, long, default_value = "approve")]
    action: String,

    /// Review body/comment
    #[arg(short, long, default_value = "")]
    body: String,

    #[arg(short = 'R', long)]
    repo: Option<String>,
}

#[derive(Args)]
struct ChecksArgs {
    /// PR number
    number: i64,

    #[arg(short = 'R', long)]
    repo: Option<String>,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct DiffArgs {
    /// PR number
    number: i64,
    #[arg(short = 'R', long)]
    repo: Option<String>,
}

impl PrCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            PrAction::List(args) => list_prs(args).await,
            PrAction::View(args) => view_pr(args).await,
            PrAction::Create(args) => create_pr(args).await,
            PrAction::Checkout(args) => checkout_pr(args).await,
            PrAction::Merge(args) => merge_pr(args).await,
            PrAction::Close(args) => set_pr_state(args.repo.as_deref(), args.number, "closed").await,
            PrAction::Reopen(args) => set_pr_state(args.repo.as_deref(), args.number, "open").await,
            PrAction::Comment(args) => comment_pr(args).await,
            PrAction::Diff(args) => diff_pr(args).await,
            PrAction::Review(args) => review_pr(args).await,
            PrAction::Checks(args) => checks_pr(args).await,
        }
    }
}

async fn list_prs(args: &ListArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let mut req = api
        .repo_list_pull_requests()
        .owner(owner)
        .repo(repo)
        .page(1)
        .limit(args.limit);

    match args.state.as_str() {
        "open" => req = req.state(gitea_api::types::RepoListPullRequestsState::Open),
        "closed" => req = req.state(gitea_api::types::RepoListPullRequestsState::Closed),
        "all" => {},
        other => eyre::bail!("Invalid state: {other}. Use open, closed, or all"),
    }

    let prs = req
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&prs)?);
        return Ok(());
    }

    if prs.is_empty() {
        eprintln!("No pull requests found");
        return Ok(());
    }

    let is_tty = atty_check();
    if is_tty {
        println!(
            "{:<6} {:<50} {:<15} {}",
            "#", "TITLE", "AUTHOR", "UPDATED"
        );
    }

    for pr in &prs {
        let number = pr.number.unwrap_or(0);
        let title = pr.title.as_deref().unwrap_or("");
        let truncated_title = if title.len() > 48 {
            format!("{}...", &title[..45])
        } else {
            title.to_string()
        };

        let author = pr
            .user
            .as_ref()
            .and_then(|u| u.login.as_deref())
            .unwrap_or("");
        let truncated_author = if author.len() > 13 {
            format!("{}...", &author[..10])
        } else {
            author.to_string()
        };

        let updated = pr
            .updated_at
            .map(|dt| relative_time(dt))
            .unwrap_or_default();

        println!(
            "{:<6} {:<50} {:<15} {}",
            number, truncated_title, truncated_author, updated
        );
    }

    Ok(())
}

async fn view_pr(args: &ViewArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let pr = api
        .repo_get_pull_request()
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
                "pr": pr,
                "comments": comments,
            });
            println!("{}", serde_json::to_string_pretty(&combined)?);
        } else {
            println!("{}", serde_json::to_string_pretty(&pr)?);
        }
        return Ok(());
    }

    let number = pr.number.unwrap_or(0);
    let title = pr.title.as_deref().unwrap_or("(no title)");
    let state = pr
        .state
        .as_ref()
        .map(|s| format!("{s:?}"))
        .unwrap_or_default()
        .to_lowercase();

    let merged = pr.merged.unwrap_or(false);
    let status = if merged { "merged".to_string() } else { state };

    println!("{title} #{number}");
    println!(
        "{status} — opened by {} {}",
        pr.user
            .as_ref()
            .and_then(|u| u.login.as_deref())
            .unwrap_or("unknown"),
        pr.created_at
            .map(|dt| relative_time(dt))
            .unwrap_or_default(),
    );

    // Head/base branches
    if let (Some(head), Some(base)) = (
        pr.head.as_ref().and_then(|h| h.label.as_deref()),
        pr.base.as_ref().and_then(|b| b.label.as_deref()),
    ) {
        println!("{head} -> {base}");
    }

    // Labels
    if !pr.labels.is_empty() {
        let label_names: Vec<&str> = pr.labels.iter().filter_map(|l| l.name.as_deref()).collect();
        println!("Labels: {}", label_names.join(", "));
    }

    // Assignees
    if !pr.assignees.is_empty() {
        let names: Vec<&str> = pr
            .assignees
            .iter()
            .filter_map(|u| u.login.as_deref())
            .collect();
        println!("Assignees: {}", names.join(", "));
    }

    // Milestone
    if let Some(ref ms) = pr.milestone {
        if let Some(ref title) = ms.title {
            println!("Milestone: {title}");
        }
    }

    // Diff stats
    if let (Some(adds), Some(dels)) = (pr.additions, pr.deletions) {
        let files = pr.changed_files.unwrap_or(0);
        println!("+{adds} -{dels} ({files} files)");
    }

    // Body
    if let Some(ref body) = pr.body {
        if !body.is_empty() {
            println!();
            println!("{body}");
        }
    }

    // URL
    if let Some(ref url) = pr.html_url {
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
            println!(
                "\n--- {} comment{} ---",
                comments.len(),
                if comments.len() == 1 { "" } else { "s" }
            );
            for c in &comments {
                let author = c
                    .user
                    .as_ref()
                    .and_then(|u| u.login.as_deref())
                    .unwrap_or("unknown");
                let when = c
                    .created_at
                    .map(|dt| relative_time(dt))
                    .unwrap_or_default();
                let body = c.body.as_deref().unwrap_or("");
                println!("\n{author} ({when}):");
                println!("{body}");
            }
        }
    } else {
        let count = pr.comments.unwrap_or(0);
        if count > 0 {
            println!(
                "\n{count} comment{} (use -c to show)",
                if count == 1 { "" } else { "s" }
            );
        }
    }

    Ok(())
}

async fn create_pr(args: &CreateArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let head = match &args.head {
        Some(h) => h.clone(),
        None => {
            let output = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .map_err(|_| eyre::eyre!("Failed to detect current branch"))?;
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
    };

    let pr = api
        .repo_create_pull_request()
        .owner(owner)
        .repo(repo)
        .body_map(|b| {
            b.title(args.title.clone())
                .body(args.body.clone())
                .base(args.base.clone())
                .head(head.clone())
        })
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    let number = pr.number.unwrap_or(0);
    let url = pr.html_url.as_deref().unwrap_or("");
    eprintln!("Created PR #{number}: {url}");
    Ok(())
}

async fn checkout_pr(args: &CheckoutArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let pr = api
        .repo_get_pull_request()
        .owner(owner)
        .repo(repo)
        .index(args.number)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    let branch = pr
        .head
        .as_ref()
        .and_then(|h| h.ref_.as_deref())
        .ok_or_else(|| eyre::eyre!("PR has no head branch"))?;

    let status = std::process::Command::new("git")
        .args(["fetch", "origin", &format!("pull/{}/head:{branch}", args.number)])
        .status()
        .map_err(|e| eyre::eyre!("git fetch failed: {e}"))?;

    if !status.success() {
        let status = std::process::Command::new("git")
            .args(["fetch", "origin", branch])
            .status()
            .map_err(|e| eyre::eyre!("git fetch failed: {e}"))?;
        if !status.success() {
            eyre::bail!("Failed to fetch PR branch");
        }
    }

    let status = std::process::Command::new("git")
        .args(["checkout", branch])
        .status()
        .map_err(|e| eyre::eyre!("git checkout failed: {e}"))?;

    if !status.success() {
        eyre::bail!("Failed to checkout branch {branch}");
    }

    eprintln!("Checked out PR #{} on branch {branch}", args.number);
    Ok(())
}

async fn merge_pr(args: &MergeArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let do_method = match args.method.as_str() {
        "merge" | "rebase" | "squash" => args.method.as_str(),
        other => eyre::bail!("Invalid merge method: {other}. Use merge, rebase, or squash"),
    };

    api.repo_merge_pull_request()
        .owner(owner)
        .repo(repo)
        .index(args.number)
        .body_map(|b| {
            b.do_(do_method.to_string())
                .delete_branch_after_merge(args.delete_branch)
        })
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    eprintln!("PR #{} merged ({do_method})", args.number);
    Ok(())
}

async fn set_pr_state(repo_opt: Option<&str>, number: i64, state: &str) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(repo_opt, &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    api.repo_edit_pull_request()
        .owner(owner)
        .repo(repo)
        .index(number)
        .body_map(|b| b.state(state.to_string()))
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    eprintln!("PR #{number} {state}");
    Ok(())
}

async fn comment_pr(args: &CommentArgs) -> Result<()> {
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

    eprintln!("Comment added to PR #{}", args.number);
    Ok(())
}

async fn review_pr(args: &ReviewArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;
    let (owner, repo) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let event = match args.action.as_str() {
        "approve" => gitea_api::types::ReviewStateType::Approved,
        "request-changes" | "request_changes" => gitea_api::types::ReviewStateType::RequestChanges,
        "comment" => gitea_api::types::ReviewStateType::Comment,
        other => eyre::bail!("Invalid review action: {other}. Use approve, request-changes, or comment"),
    };

    let action_str = args.action.clone();
    api.repo_create_pull_review()
        .owner(owner)
        .repo(repo)
        .index(args.number)
        .body_map(move |b| b.body(args.body.clone()).event(event.clone()))
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    eprintln!("Review submitted on PR #{}: {action_str}", args.number);
    Ok(())
}

async fn checks_pr(args: &ChecksArgs) -> Result<()> {
    let config = Config::load()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;

    // Use reqwest to get commit statuses for the PR's head SHA
    let api = config.client()?;
    let pr = api
        .repo_get_pull_request()
        .owner(&repo_info.owner)
        .repo(&repo_info.name)
        .index(args.number)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    let sha = pr
        .head
        .as_ref()
        .and_then(|h| h.sha.as_deref())
        .ok_or_else(|| eyre::eyre!("PR has no head SHA"))?;

    // Get combined status for the SHA
    let combined = api
        .repo_get_combined_status_by_ref()
        .owner(&repo_info.owner)
        .repo(&repo_info.name)
        .ref_(sha)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{e}"))?
        .into_inner();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&combined)?);
        return Ok(());
    }

    let state = combined.state.as_ref().map(|s| format!("{s:?}")).unwrap_or_else(|| "unknown".to_string()).to_lowercase();
    println!("Overall: {state}");

    if combined.statuses.is_empty() {
        println!("No checks found");
    }
    for s in &combined.statuses {
        let context = s.context.as_deref().unwrap_or("");
        let s_status = s.status.as_ref().map(|st| format!("{st:?}")).unwrap_or_default().to_lowercase();
        let desc = s.description.as_deref().unwrap_or("");
        println!("  {s_status:<10} {context} — {desc}");
    }

    Ok(())
}

async fn diff_pr(args: &DiffArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;

    let repo_info = repo::resolve_repo(args.repo.as_deref(), &config.url)?;

    // The diff endpoint returns plain text, not JSON.
    // Use raw_get since the typed client expects JSON responses.
    let path = format!(
        "repos/{}/{}/pulls/{}.diff",
        repo_info.owner, repo_info.name, args.number,
    );
    let text = api.raw_get(&path).await.map_err(|e| eyre::eyre!("{e}"))?;
    print!("{text}");
    Ok(())
}
