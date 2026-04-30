# `gt runner` subcommand — design

**Date:** 2026-04-30
**Bug:** [bugs/002-no-runner-subcommand.md](../../../bugs/002-no-runner-subcommand.md)

## Goal

Add a first-class `gt runner` subcommand for managing Gitea Actions runners across admin, organization, and repository scopes. Today users fall back to `gt api` and must remember endpoint paths, HTTP verbs, and required scopes — the named ops case (drawbar runner registration on a fresh Gitea instance) is the most common example.

## Non-goals

- **User-scope runners** (`/user/actions/runners/*`). Rarely used; adds a fourth code path. Easy to add later.
- **`gt runner edit`** (PATCH `updateAdminRunner` / `updateOrgRunner` / `updateRepoRunner`). Use case (label changes, disable without delete) is real but rare. Follow-up.
- **`gt runner status <id>`** as a separate command. The bug names it but `ActionRunner` exposes no `last_online` / `version` / live-state fields beyond the simple `status` string. Rolled into `view`.
- **Pagination flags** (`--limit`, `--page`). API default is 30; no real instance has more.
- **404-on-wrong-scope auto-search.** Considered and rejected — masks the user's mental model of which scope owns a runner.

## Command surface

```
gt runner list [--admin | --org NAME | -R OWNER/REPO]
gt runner view <id> [--admin | --org NAME | -R OWNER/REPO]
gt runner delete <id> [-y | --yes] [--admin | --org NAME | -R OWNER/REPO]
gt runner registration-token [--admin | --org NAME | -R OWNER/REPO]
```

### Scope resolution

Scope flags `--admin`, `--org NAME`, and `-R OWNER/REPO` are mutually exclusive. With none given, fall back to repo auto-detection from the git remote (matching `gt run`, `gt secret`, `gt variable`).

If repo detection fails (no git remote matching the configured Gitea instance): error message includes the standard `repo::resolve_repo` failure (`No git remote matching Gitea instance ... found.`) followed by `Use --admin, --org NAME, or -R OWNER/REPO to specify scope.`

Precedence (highest first):

1. `--admin`
2. `--org NAME`
3. `-R OWNER/REPO` (explicit)
4. `GITEA_REPO` env var
5. Git remote auto-detection

(Steps 3–5 are handled by the existing `repo::resolve_repo` helper.)

### `gt runner list`

TTY (non-JSON) output:

```
ID    NAME                STATUS   LABELS                         FLAGS
3     drawbar-host-1      online   self-hosted,linux,x64
7     ephem-builder       offline  self-hosted,docker             ephemeral
12    legacy              online   self-hosted,linux              busy
```

- `NAME` truncated to 20 chars with `...` overflow.
- `LABELS` is comma-joined `runner.labels[].name`, truncated to 30 chars.
- `FLAGS` is comma-joined from boolean fields that are `true`: `busy`, `disabled`, `ephemeral`. Empty (the common case) shows nothing.
- Empty list: `eprintln!("No runners found")`, exit 0. Mirrors `gt secret list`.

JSON output via existing `crate::json::JsonArgs` + `write_json` helper. The `runners[]` array is lifted out of `ActionRunnersResponse` before serializing (mirrors `gt run list`).

```rust
const RUNNER_FIELDS: &[&str] = &[
    "id", "name", "status", "busy", "disabled", "ephemeral", "labels",
];
```

### `gt runner view <id>`

```
drawbar-host-1 (#3)
Status: online
Labels: self-hosted, linux, x64
Flags: busy
```

`Flags:` line is omitted when no flags are true. `--json` flag (bool) emits the raw `ActionRunner` JSON, matching `gt run view`.

### `gt runner delete <id>`

1. Fetch the runner first (`get_admin_runner` / `get_org_runner` / `get_repo_runner`) to learn its name.
2. If TTY and not `--yes`: prompt via `inquire::Confirm` with default `false`:
   `Delete runner #<id> "<name>"? [y/N]`
   Bail on `false`.
3. Issue the delete. Print `Deleted runner #<id>` to stderr.
4. Non-TTY without `--yes`: bail with `error: refusing to delete without -y/--yes (stdin is not a TTY)`. Prevents script accidents.

### `gt runner registration-token`

1. POST to the appropriate endpoint:
   - admin: `admin_create_runner_registration_token()`
   - org: `org_create_runner_registration_token().org(o)`
   - repo: `repo_create_runner_registration_token().owner(o).repo(r)`
2. Response shape: `{ "token": "..." }` (typed wrapper from progenitor).
3. Print just the token + newline to stdout. Nothing else. Pipe-friendly:
   `gt runner registration-token --admin | xargs -I{} act_runner register --token {} ...`

## Architecture

### File footprint

- **New:** `gt/src/runner.rs` (~300 lines)
- **Modified:** `gt/src/main.rs` (3 edits: `mod runner;`, `Command::Runner` variant, `run_app` match arm)
- **Unchanged:** `gitea-api/`. The generated client already has every needed method.

### Module shape (`gt/src/runner.rs`)

```rust
#[derive(Args)]
pub struct RunnerCommand {
    #[command(flatten)]
    repo: repo::RepoArgs,           // global -R inherited from RepoArgs
    #[command(subcommand)]
    action: RunnerAction,
}

#[derive(Subcommand)]
enum RunnerAction {
    List(ListArgs),
    View(ViewArgs),
    Delete(DeleteArgs),
    RegistrationToken(TokenArgs),
}

#[derive(Args)]
struct ScopeArgs {
    /// Instance-wide (admin only)
    #[arg(long, conflicts_with = "org")]
    admin: bool,
    /// Organization name
    #[arg(long)]
    org: Option<String>,
}
```

Each action's `Args` struct flattens `ScopeArgs`. `-R` is already global from `RepoArgs` on the parent, so it doesn't need to be redeclared and clap won't see a conflict between `-R` and `--admin`/`--org` at parse time — precedence is resolved in `Scope::resolve` below.

### Scope helper

```rust
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
    let info = repo::resolve_repo(repo_args.repo.as_deref(), config_url)?;
    Ok(Scope::Repo { owner: info.owner, name: info.name })
}
```

### API dispatch pattern

Each action runs an inline 3-arm match — no abstraction layer. Matches existing project style (`secret.rs`, `variable.rs`, `run.rs`).

```rust
async fn list_runners(
    scope: &Scope,
    api: &Gitea,
    args: &ListArgs,
) -> Result<()> {
    let resp = match scope {
        Scope::Admin => api
            .get_admin_runners()
            .send()
            .await
            .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
            .into_inner(),
        Scope::Org(o) => api
            .get_org_runners()
            .org(o)
            .send()
            .await
            .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
            .into_inner(),
        Scope::Repo { owner, name } => api
            .get_repo_runners()
            .owner(owner)
            .repo(name)
            .send()
            .await
            .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
            .into_inner(),
    };
    // print resp.runners
}
```

The 3-arm match repeats once per action (×4 actions). The repetition is tolerable — extracting a helper would force closure-type unification across distinct progenitor builder types and net to more code.

## Data shapes

From `gitea-api/openapi.v1.json`:

```
ActionRunner {
    id:        int64
    name:      string
    status:    string         // "online" / "offline" / etc — values undocumented
    busy:      bool
    disabled:  bool
    ephemeral: bool
    labels:    [{ id, name, type }]
}

ActionRunnersResponse {
    runners:     [ActionRunner]
    total_count: int64
}

RegistrationToken { token: string }
```

## API endpoint mapping

| `gt` command | Endpoint | Verb |
|---|---|---|
| `runner list --admin` | `/admin/actions/runners` | GET |
| `runner list --org X` | `/orgs/X/actions/runners` | GET |
| `runner list -R o/r` | `/repos/o/r/actions/runners` | GET |
| `runner view <id> --admin` | `/admin/actions/runners/{id}` | GET |
| `runner view <id> --org X` | `/orgs/X/actions/runners/{id}` | GET |
| `runner view <id> -R o/r` | `/repos/o/r/actions/runners/{id}` | GET |
| `runner delete <id> --admin` | `/admin/actions/runners/{id}` | DELETE |
| `runner delete <id> --org X` | `/orgs/X/actions/runners/{id}` | DELETE |
| `runner delete <id> -R o/r` | `/repos/o/r/actions/runners/{id}` | DELETE |
| `runner registration-token --admin` | `/admin/actions/runners/registration-token` | **POST** |
| `runner registration-token --org X` | `/orgs/X/actions/runners/registration-token` | **POST** |
| `runner registration-token -R o/r` | `/repos/o/r/actions/runners/registration-token` | **POST** |

The `registration-token` endpoint is **POST** (verified against Gitea source `routers/api/v1/api.go` and the OpenAPI spec). The Gitea source uses `m.Post("/registration-token", ...)` for all four scopes.

## Error handling

All API errors flow through the existing `gitea_api::GiteaError` pattern (see `gt/src/run.rs`). 401 and 403 hints come from `main.rs`'s post-result inspection — no new code needed in `runner.rs`. A 404 from "runner not in this scope" gets the generic Gitea error message, matching how `gt secret delete <wrong-name>` behaves today.

## Testing

Unit tests in `runner.rs` `#[cfg(test)] mod tests`:

1. **`resolve_scope` precedence:** `--admin` wins over `--org`; `--org X` wins over repo detection; absent flags fall through to `repo::resolve_repo`. Pure function, no mocking.
2. **Clap mutual exclusion:** `gt runner list --admin --org foo` fails to parse with a clap error. Tested via `assert_cmd` (already a dev-dep).
3. **Output formatting helpers:** the `FLAGS` column (empty when all false; comma-joined when multiple true) and label truncation. Pure functions.

No live-API tests — the rest of the codebase doesn't have them, and mocking progenitor builders costs more than v1 is worth.

## Acceptance (from bug 002)

- [ ] `gt runner list` works in a repo-detected context.
- [ ] `gt runner registration-token --admin` returns a token that can register a runner.
- [ ] 403 from a non-admin token surfaces a hint. The existing `main.rs:171-173` hint reads `"hint: you don't have permission for this operation"` — generic, not admin-specific. Bug 003 (separate) proposes upgrading the 403 hint to name the missing scope. This spec relies on whatever `main.rs` does today; tightening is bug 003's concern.

## Open verifications during implementation

- Confirm progenitor-generated method names match the OpenAPI `operationId`s as expected (snake_case): `get_admin_runners`, `get_org_runners`, `get_repo_runners`, `get_admin_runner`, `get_org_runner`, `get_repo_runner`, `delete_admin_runner`, `delete_org_runner`, `delete_repo_runner`, `admin_create_runner_registration_token`, `org_create_runner_registration_token`, `repo_create_runner_registration_token`.
- Confirm `RegistrationToken` response wrapper exposes `token` field directly (likely `.token` on the typed body, or `.into_inner().token`).
