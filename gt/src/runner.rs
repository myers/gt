use clap::{Args, Subcommand};
use eyre::Result;
use url::Url;

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
    List,
}

impl RunnerCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            RunnerAction::List => {
                eyre::bail!("not implemented yet");
            }
        }
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
}
