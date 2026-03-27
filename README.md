# gt — Gitea CLI

> **Early Alpha** — written by a coding agent, has not been used yet. YOU HAVE BEEN WARNED.

A command-line tool for Gitea, modeled after GitHub's `gh` CLI. Built in Rust.

## Commands

| Command | Description |
|---------|-------------|
| `gt issue` | Manage issues (list, create, view, edit, close, comment, lock, pin) |
| `gt pr` | Manage pull requests (list, create, view, merge, diff, checkout) |
| `gt repo` | Manage repositories (list, create, clone, fork, edit, delete, archive) |
| `gt label` | Manage labels (list, create, edit, delete, clone) |
| `gt milestone` | Manage milestones |
| `gt release` | Manage releases (list, create, edit, delete, with asset upload) |
| `gt project` | Manage project boards |
| `gt run` | Manage Actions workflow runs |
| `gt workflow` | Manage Actions workflows (list, enable, disable, run) |
| `gt search` | Search repos, issues, users |
| `gt secret` | Manage repository secrets |
| `gt variable` | Manage repository variables |
| `gt notification` | Manage notifications |
| `gt org` | Manage organizations |
| `gt ssh-key` | Manage SSH keys |
| `gt gpg-key` | Manage GPG keys |
| `gt alias` | Manage command aliases (including shell aliases with `!` prefix) |
| `gt status` | Dashboard: notifications, assigned issues, review requests |
| `gt auth` | Authentication (login, logout, status) |
| `gt api` | Make authenticated API requests |
| `gt browse` | Open in browser |
| `gt completion` | Generate shell completions (bash, zsh, fish) |

## Features

- `--json field,field` + `--jq` on list commands
- `--body-file` with auto-upload of local image/file attachments
- Interactive create flows (issues, PRs, releases) with `$EDITOR` support
- Command aliases (`gt alias set co "pr checkout"`, shell aliases with `!`)
- Pagination, global `-R owner/repo` flag

## Workspace

Cargo workspace with two crates:

- **`gitea-api`** — API client generated from Gitea's OpenAPI 3.0 spec using [progenitor](https://github.com/oxidecomputer/progenitor)
- **`gt`** — CLI binary

## Building

```bash
cargo build --release
```

## License

MIT
