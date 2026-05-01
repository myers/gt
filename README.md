# gt — Gitea CLI

> **Early Alpha** — written by a coding agent, has not been used yet. YOU HAVE BEEN WARNED.

A command-line tool for Gitea, modeled after GitHub's `gh` CLI. Built in Rust.

This attempts to be a faithful copy of `gh` for GitHub. Any places it differs should be considered a bug.

## Install

```bash
cargo install --git https://github.com/myers/gt gt
```

## Setup

```bash
gt auth login --url https://your-instance --token YOUR_TOKEN
gt auth setup-git
```

The first command saves your Gitea instance URL and API token. The second configures git to authenticate using `gt`, so clone/push/pull just work.

You can generate a token at `https://your-instance/user/settings/applications`.

## Usage

```
$ gt --help
Gitea CLI

Usage: gt [OPTIONS] <COMMAND>

Commands:
  issue         Manage issues
  pr            Manage pull requests
  repo          Manage repositories
  label         Manage labels
  milestone     Manage milestones
  release       Manage releases
  project       Manage projects
  run           Manage Actions workflow runs
  runner        Manage Actions runners
  search        Search repos, issues, users
  secret        Manage repository secrets
  variable      Manage repository variables
  notification  Manage notifications
  workflow      Manage Actions workflows
  org           Manage organizations
  ssh-key       Manage your SSH keys
  gpg-key       Manage your GPG keys
  alias         Manage command aliases
  status        Show status dashboard (notifications, assigned, review requests)
  auth          Authentication commands
  config        Manage configuration
  browse        Open in browser
  api           Make an authenticated API request
  completion    Generate shell completions
  help          Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...    Print HTTP request/response transcripts on stderr. Repeat for more detail (`-vv` includes request/response bodies). Tokens are masked unless `--show-secrets` is passed
      --show-secrets  With `-v`, print Authorization/Cookie header values unmasked. Off by default — paste-into-chat safety
  -h, --help          Print help
  -V, --version       Print version
```

## License

MIT
