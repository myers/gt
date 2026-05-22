# gt CLI — TODO

Command roadmap modeled after `gh` (GitHub CLI), adapted for Gitea.

## Currently Implemented

- [x] `gt issue list` — list issues (state filter, limit, JSON output, table output)
- [x] `gt issue view <number>` — view issue details (comments via `-c`, JSON output)
- [x] Config system — `~/.config/gt/config.toml`, env vars `GITEA_URL` / `GITEA_TOKEN`

## Priority 1: Core Commands

### Git-remote-aware repo detection
Foundation for all commands — auto-detect repo from git remote so users
don't need `-R owner/repo` on every command.

- [x] Auto-detect `owner/repo` from git remote `origin`
- [x] Match remote URL to configured Gitea instance URL
- [x] Support `-R owner/repo` override (like `gh -R`)
- [x] Support `GITEA_REPO` env var

### `gt api` — Raw API access (backdoor to everything)
The universal escape hatch. Lets users hit any Gitea API endpoint
without waiting for dedicated subcommands.

- [x] `gt api <endpoint>` — GET request to Gitea API
- [x] `gt api -X POST <endpoint>` — specify HTTP method
- [x] `gt api -f key=value` — add form/JSON fields
- [x] `gt api -F key=value` — typed fields (int, bool, @file)
- [x] `gt api -H key:value` — custom headers
- [x] `gt api --jq <expr>` — filter JSON output with jq syntax
- [x] `gt api --paginate` — follow pagination links
- [x] `gt api -i` — include response headers
- [x] `{owner}` / `{repo}` placeholder expansion from git remote

### `gt pr` — Pull request management
- [x] `gt pr list` — list PRs (state, label, assignee filters)
- [x] `gt pr view <number>` — view PR details, diff stats, CI status
- [x] `gt pr create` — create PR (title, body, base branch, head branch auto-detected)
- [x] `gt pr checkout <number>` — fetch and checkout PR branch
- [x] `gt pr merge <number>` — merge PR (merge/rebase/squash, --delete-branch)
- [x] `gt pr close <number>` — close PR
- [x] `gt pr reopen <number>` — reopen PR
- [x] `gt pr comment <number>` — add comment
- [x] `gt pr diff <number>` — view diff
- [x] `gt pr review <number>` — approve/request changes
- [x] `gt pr checks <number>` — show CI status

### `gt issue` — Complete issue management
- [x] `gt issue create` — create issue (title, body, assignees)
- [x] `gt issue close <number>` — close issue
- [x] `gt issue reopen <number>` — reopen issue
- [x] `gt issue comment <number>` — add comment
- [x] `gt issue edit <number>` — edit title, body
- [x] `gt issue status` — show open issues in repo

## Priority 2: Repository & Navigation

### `gt repo` — Repository management
- [x] `gt repo view` — show repo info (description, stats, default branch)
- [x] `gt repo clone <owner/repo>` — clone a Gitea repo
- [x] `gt repo list` — list repos for user/org
- [x] `gt repo create` — create new repo
- [x] `gt repo fork` — fork a repo

### `gt browse` — Open in browser
- [x] `gt browse` — open repo in browser
- [x] `gt browse <number>` — open issue/PR in browser
- [x] `gt browse --settings` — open repo settings

## Priority 3: Projects, Labels, Milestones, Releases

### `gt project` — Project board management
- [x] `gt project list` — list projects
- [x] `gt project view <id>` — view project details
- [x] `gt project create` — create project
- [x] `gt project close` / `gt project reopen`
- [x] `gt project column list` — list columns
- [x] `gt project column create` — add column

### `gt label` — Label management
- [x] `gt label list` — list labels
- [x] `gt label create` — create label (name, color, description)
- [x] `gt label edit` — edit label
- [x] `gt label delete` — delete label

### `gt milestone` — Milestone management
- [x] `gt milestone list` — list milestones
- [x] `gt milestone create` — create milestone
- [x] `gt milestone view` — view milestone details
- [x] `gt milestone close` / `gt milestone reopen`

### `gt release` — Release management
- [x] `gt release list` — list releases
- [x] `gt release create` — create release with assets
- [x] `gt release view` — view release
- [x] `gt release download` — download release assets
- [x] `gt release delete` — delete release

## Priority 4: Actions, Org

### `gt run` — Actions/CI
- [x] `gt run list` — list workflow runs
- [x] `gt run view <id>` — view run details and logs
- [x] `gt run rerun <id>` — rerun a workflow
- [x] `gt run watch [<id>]` — poll until complete (`--exit-status`, `--compact`, `--interval`)
- [x] `gt run download <id>` — download artifacts (`-d <dir>`)
- [ ] `gt run cancel <id>` — cancel an in-progress run (mirrors `gh run cancel`)
- [ ] `gt run delete <id>` — delete a run from history (mirrors `gh run delete`)

### `gt secret` — Actions secrets (gap, hit during 2026-05 drawbar eval)
Mirrors `gh secret`. Today the only path is `gt api --method PUT
repos/{owner}/{repo}/actions/secrets/<NAME> --field 'data=<value>'`,
which is awkward enough that it surprised me twice in one session.

- [ ] `gt secret list` — list repo/org actions secrets (names only, never values)
- [ ] `gt secret set <NAME>` — create or update; read value from `--body`, `--body-file`, or stdin
- [ ] `gt secret delete <NAME>` — remove a secret
- [ ] `--org <ORG>` flag so each subcommand can target org secrets instead of repo secrets

### `gt org` — Organization management
- [x] `gt org list` — list orgs the user belongs to
- [x] `gt org view <name>` — view org details

## Priority 5: Auth, Config, Completions

### `gt auth` — Authentication
- [x] `gt auth login` — interactive login (token input or browser OAuth)
- [x] `gt auth status` — show current auth state
- [x] `gt auth logout` — remove credentials
- [x] Multi-instance support (multiple Gitea servers via `[servers.NAME]` + `GITEA_SERVER` env)

### `gt config` — Configuration
- [x] `gt config get <key>` — read config value
- [x] `gt config set <key> <value>` — write config value
- [x] `gt config list` — show all config

### `gt completion` — Shell completions
- [x] `gt completion bash`
- [x] `gt completion zsh`
- [x] `gt completion fish`

## Architecture Notes

### `gt api` design
The `api` command is the universal escape hatch. It should:
1. Accept any Gitea API path (e.g., `repos/{owner}/{repo}/issues`)
2. Expand `{owner}` and `{repo}` from git remote
3. Handle pagination transparently with `--paginate`
4. Support jq-style filtering for scripting
5. Use the same auth config as other commands

This means every Gitea API feature is accessible immediately, even before
we write a dedicated subcommand.

### Output conventions
- **Table output** (default for TTY): human-readable columns, truncated to terminal width
- **JSON output** (`--json`): machine-readable, full data
- **jq filtering** (`--jq`): for scripting pipelines
- All commands should support `--json` for scriptability

### Testing
Set up after repo detection is implemented:

**Unit tests** (`#[cfg(test)]` modules in each source file):
- Config parsing — TOML loading, env var overrides, missing values
- Repo detection — parse git remote URLs (SSH, HTTPS, `git@`, custom ports) into owner/repo
- Output formatting — relative time, table truncation
- API command arg parsing — field types, placeholder expansion

**Integration tests** (`gt/tests/integration.rs`, uses `assert_cmd` crate):
- Start a real Gitea instance (temp dir, SQLite, ephemeral port)
- Create test data via API (user, repo, issues, labels, project)
- Run `gt` binary and assert on stdout/stderr/exit code
- Test commands: `gt issue list`, `gt issue view`, `gt api`, `gt pr list`
- Tear down after each test

**Workflow**: run `cargo test` and commit only after ALL tests pass (including
pre-existing ones). Fix any pre-existing test failures before moving on.

### Config hierarchy
1. CLI flags (highest priority)
2. Environment variables (`GITEA_URL`, `GITEA_TOKEN`, `GITEA_REPO`)
3. Repo-local config (`.gt.toml` in repo root — future)
4. User config (`~/.config/gt/config.toml`)
