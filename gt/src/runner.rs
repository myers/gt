use clap::{Args, Subcommand};
use eyre::Result;
use gitea_api::types::ActionRunner;
use url::Url;

use crate::config::Config;
use crate::issues::atty_check;
use crate::json::{JsonArgs, write_json};
use crate::repo;

#[derive(Args, Default)]
struct ScopeArgs {
    /// Instance-wide (admin only)
    #[arg(long, conflicts_with = "org")]
    admin: bool,

    /// Organization name
    #[arg(long, value_name = "NAME")]
    org: Option<String>,
}

#[derive(Debug)]
enum Scope {
    Admin,
    Org(String),
    Repo { owner: String, name: String },
}

fn resolve_scope(
    scope: &ScopeArgs,
    repo_args: &repo::RepoArgs,
    config_url: &Url,
) -> Result<Scope> {
    if scope.admin {
        return Ok(Scope::Admin);
    }
    if let Some(org) = &scope.org {
        return Ok(Scope::Org(org.clone()));
    }
    let info = repo::resolve_repo(repo_args.repo.as_deref(), config_url).map_err(|e| {
        eyre::eyre!("{e}\nUse --admin, --org NAME, or -R OWNER/REPO to specify scope.")
    })?;
    Ok(Scope::Repo {
        owner: info.owner,
        name: info.name,
    })
}

#[derive(Args)]
pub struct RunnerCommand {
    #[command(flatten)]
    repo: repo::RepoArgs,

    #[command(subcommand)]
    action: RunnerAction,
}

#[derive(Subcommand)]
enum RunnerAction {
    /// List runners
    List(ListArgs),
}

#[derive(Args)]
struct ListArgs {
    #[command(flatten)]
    scope: ScopeArgs,

    #[command(flatten)]
    json: JsonArgs,
}

impl RunnerCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            RunnerAction::List(args) => list_runners(&self.repo, args).await,
        }
    }
}

const RUNNER_FIELDS: &[&str] = &[
    "id",
    "name",
    "status",
    "busy",
    "disabled",
    "ephemeral",
    "labels",
];

async fn list_runners(repo_args: &repo::RepoArgs, args: &ListArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;
    let scope = resolve_scope(&args.scope, repo_args, &config.url)?;

    let runners: Vec<ActionRunner> = match &scope {
        Scope::Admin => api
            .get_admin_runners()
            .send()
            .await
            .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
            .into_inner()
            .runners,
        Scope::Org(o) => api
            .get_org_runners()
            .org(o.clone())
            .send()
            .await
            .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
            .into_inner()
            .runners,
        Scope::Repo { owner, name } => api
            .get_repo_runners()
            .owner(owner.clone())
            .repo(name.clone())
            .send()
            .await
            .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
            .into_inner()
            .runners,
    };

    if args.json.is_json() {
        return write_json(&args.json, &runners, RUNNER_FIELDS);
    }

    if runners.is_empty() {
        eprintln!("No runners found");
        return Ok(());
    }

    let is_tty = atty_check();
    if is_tty {
        println!(
            "{:<6} {:<20} {:<8} {:<30} {}",
            "ID", "NAME", "STATUS", "LABELS", "FLAGS"
        );
    }

    for r in &runners {
        let id = r.id.unwrap_or(0);
        let name = truncate(r.name.as_deref().unwrap_or(""), 20);
        let status = r.status.as_deref().unwrap_or("");
        let labels = truncate(&labels_string(r), 30);
        let flags = flags_string(r);
        println!("{id:<6} {name:<20} {status:<8} {labels:<30} {flags}");
    }

    Ok(())
}

fn flags_string(r: &ActionRunner) -> String {
    let mut out = Vec::new();
    if r.busy.unwrap_or(false) {
        out.push("busy");
    }
    if r.disabled.unwrap_or(false) {
        out.push("disabled");
    }
    if r.ephemeral.unwrap_or(false) {
        out.push("ephemeral");
    }
    out.join(",")
}

fn labels_string(r: &ActionRunner) -> String {
    r.labels
        .iter()
        .filter_map(|l| l.name.as_deref())
        .collect::<Vec<_>>()
        .join(",")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let cut = max.saturating_sub(3);
        format!("{}...", &s[..cut])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_url() -> Url {
        Url::parse("https://gitea.example.com").unwrap()
    }

    #[test]
    fn resolve_scope_admin_wins_over_org() {
        let scope = ScopeArgs {
            admin: true,
            org: Some("ignored".into()),
        };
        let repo = repo::RepoArgs::default();
        let r = resolve_scope(&scope, &repo, &fake_url()).unwrap();
        assert!(matches!(r, Scope::Admin));
    }

    #[test]
    fn resolve_scope_org_when_no_admin() {
        let scope = ScopeArgs {
            admin: false,
            org: Some("acme".into()),
        };
        let repo = repo::RepoArgs::default();
        let r = resolve_scope(&scope, &repo, &fake_url()).unwrap();
        assert!(matches!(r, Scope::Org(o) if o == "acme"));
    }

    #[test]
    fn resolve_scope_repo_explicit() {
        let scope = ScopeArgs {
            admin: false,
            org: None,
        };
        let repo = repo::RepoArgs {
            repo: Some("alice/proj".into()),
        };
        let r = resolve_scope(&scope, &repo, &fake_url()).unwrap();
        match r {
            Scope::Repo { owner, name } => {
                assert_eq!(owner, "alice");
                assert_eq!(name, "proj");
            }
            other => panic!("expected Repo, got {other:?}"),
        }
    }

    #[test]
    fn flags_string_empty_when_all_false() {
        let r = ActionRunner {
            busy: Some(false),
            disabled: Some(false),
            ephemeral: Some(false),
            id: Some(1),
            labels: vec![],
            name: Some("r".into()),
            status: Some("online".into()),
        };
        assert_eq!(flags_string(&r), "");
    }

    #[test]
    fn flags_string_lists_true_flags_in_order() {
        let r = ActionRunner {
            busy: Some(true),
            disabled: Some(false),
            ephemeral: Some(true),
            id: Some(1),
            labels: vec![],
            name: Some("r".into()),
            status: Some("online".into()),
        };
        assert_eq!(flags_string(&r), "busy,ephemeral");
    }

    #[test]
    fn labels_string_joins_with_comma() {
        use gitea_api::types::ActionRunnerLabel;
        let r = ActionRunner {
            busy: Some(false),
            disabled: Some(false),
            ephemeral: Some(false),
            id: Some(1),
            labels: vec![
                ActionRunnerLabel {
                    id: Some(1),
                    name: Some("self-hosted".into()),
                    type_: Some("custom".into()),
                },
                ActionRunnerLabel {
                    id: Some(2),
                    name: Some("linux".into()),
                    type_: Some("custom".into()),
                },
            ],
            name: Some("r".into()),
            status: Some("online".into()),
        };
        assert_eq!(labels_string(&r), "self-hosted,linux");
    }

    #[test]
    fn truncate_short_returns_unchanged() {
        assert_eq!(truncate("abc", 10), "abc");
    }

    #[test]
    fn truncate_long_appends_ellipsis() {
        assert_eq!(truncate("abcdefghij", 6), "abc...");
    }
}
