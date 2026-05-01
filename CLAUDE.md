# gt — project notes for Claude

## Versioning

Bump `gt/Cargo.toml`'s `version` on every commit, as you (Claude) feel
appropriate:

- patch (`0.2.0` → `0.2.1`): bug fix, doc tweak, internal refactor with no
  user-visible change.
- minor (`0.2.0` → `0.3.0`): new subcommand, new flag, new behavior, anything
  the user could notice in `--help` output or tab completion.
- major: breaking changes to existing flags / output formats / config schema.

Version + `--version` output also embeds the short git SHA and build date
(see `gt/build.rs`); no need to touch those by hand.

`gitea-api/Cargo.toml` doesn't get bumped per-commit — only when its public
surface meaningfully changes.
