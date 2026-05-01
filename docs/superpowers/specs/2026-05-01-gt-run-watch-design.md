# `gt run watch` — gh-parity polish

## Goal

Bring `gt run watch` to behavioral parity with `gh run watch`. The README declares
"any place [`gt`] differs [from `gh`] should be considered a bug," so the
target behavior is defined by `gh run watch`'s actual contract, not by what's
already implemented.

## Current state

`gt run watch <id>` already exists (`gt/src/run.rs::watch_run`). It:

- Polls `GET /repos/{owner}/{repo}/actions/runs/{id}` and the per-run jobs
  endpoint every N seconds.
- Renders a status table showing job icons (`✓ ✗ ⊘ ● ○`) and per-job status.
- Repaints by clearing the entire screen (`\x1b[2J\x1b[H`).
- Exits when status is terminal. Exits **1** on conclusion `failure` (always),
  Ok otherwise.
- Defaults `--interval` to 5s.
- Requires `<id>` as a positional arg.

## gh contract (from `gh run watch --help`)

- `gh run watch [<run-id>] [flags]` — id is **optional**; missing id triggers
  a picker.
- `--exit-status` opt-in flag — exits 0 on completion regardless of conclusion
  unless this is set, in which case failure conclusions exit non-zero.
- `--compact` filters to relevant/failed steps only (omits successful steps).
- `--interval` default is **3s**.
- Refresh redraws in place using cursor-up + line-clear, preserving scrollback.
- No-arg form's picker shows in-progress runs first; falls back to recent
  completed runs if none are in-progress.
- No log fetching at terminal — logs are a separate concern handled by
  `gh run view --log`.

## Gaps to close

1. Make `<id>` optional. When missing, show a picker.
2. Picker source: in-progress runs first, recent runs as fallback.
3. Change default `--interval` from 5s to 3s.
4. Add `--compact` to hide successful steps. Default shows all.
5. Add `--exit-status`. Change default exit behavior to exit 0 on terminal,
   regardless of conclusion. Only `--exit-status` makes failure non-zero. This
   is a **breaking change** to the current `gt` exit-code behavior, but the
   current behavior contradicts gh-parity.
6. Replace full-screen clear with cursor-relative redraw that preserves
   scrollback.

## Non-goals

- Mid-run log streaming or log dumping at terminal.
- A new `--log` flag — that lives on `gt run view`, matching gh.
- Token/auth changes — uses the same client.

## Design

### CLI surface

```rust
struct WatchArgs {
    /// Run ID. If omitted, prompt to pick from in-progress runs.
    id: Option<i64>,

    /// Refresh interval in seconds. Default 3 to match `gh run watch`.
    #[arg(short = 'i', long, default_value = "3")]
    interval: u64,

    /// Hide successful steps; show only relevant/failed steps.
    #[arg(long)]
    compact: bool,

    /// Exit non-zero if the run's conclusion is a failure. By default,
    /// `gt run watch` exits 0 once the run reaches a terminal state,
    /// regardless of conclusion (matches `gh run watch`).
    #[arg(long = "exit-status")]
    exit_status: bool,
}
```

### Picker (no-arg form)

When `args.id.is_none()`:

1. Fetch one page of recent runs (`GET .../actions/runs?limit=30`).
2. Partition into `in_progress` (status ∈ {`waiting`, `queued`, `in_progress`})
   and `recent` (everything else).
3. If both are empty: print `No workflow runs found` and exit 0 (no error;
   nothing to do).
4. Build the picker list — in-progress runs first (newest → oldest), then a
   visual separator, then recent runs (newest → oldest, capped at 10).
5. Use `inquire::Select` (already a dependency, used elsewhere in the codebase)
   with each option labeled `<status-icon> #<id> <branch> <display_title>`.
6. If the user cancels (Ctrl-C in the picker), exit 130.

### Step-level rendering and compact mode

The current implementation only renders jobs (one icon + name per job). gh
renders **steps** within each job, and `--compact` filters at the step level.
Gitea's `list_workflow_run_jobs` response includes a `steps: Vec<ActionWorkflowStep>`
field per job, so step-level rendering is achievable through the typed
client.

This spec replaces the current `raw_get(...jobs)` / `serde_json::Value` walk
with the typed `api.list_workflow_run_jobs()` builder so we can iterate
`job.steps` directly with proper types.

Default rendering:

```
Run #42 — feat: add gt run watch
Status: in_progress

  ✓ build
    ✓ checkout
    ✓ install deps
    ✓ cargo test
  ● lint (in_progress)
    ✓ checkout
    ● clippy
  ○ release (queued)
```

`--compact` rules:

- Job line shown unless the job is fully successful AND all its steps are
  successful. (A successful job with no failed steps is hidden entirely.)
- Step line shown when the step's conclusion is in {`failure`, `cancelled`,
  `timed_out`, `action_required`} OR its status is `in_progress` / `queued`.
  Successful steps are hidden.
- Run header (run id + title + overall status) always shown.

If `--compact` filtering hides every job, render a single line:
`(all steps passing so far)` so the screen isn't blank.

### Exit codes

| Condition | Exit code |
|---|---|
| Run reached terminal state | 0 (default) |
| Run reached terminal state with conclusion ∈ {failure, cancelled, timed_out, action_required} AND `--exit-status` was set | 1 |
| Network error / 4xx / 5xx during polling | 1 |
| User cancels picker (Ctrl-C) | 130 |

Note: this **changes** today's behavior. Today, `gt run watch <id>` exits 1 on
failure unconditionally. After this change, that's only true with `--exit-status`.
This is the right call because the README treats gh divergence as a bug.

### Scrollback-preserving redraw

Track the number of lines printed each poll cycle. On the next cycle, before
printing, emit `\x1b[<n>F` (move cursor up `n` lines to start of line) and
`\x1b[J` (clear from cursor to end of screen). On the first cycle nothing is
emitted (so the cursor is at the bottom of whatever was already there).

When stdout is not a TTY (`!atty_check()` already exists in the codebase via
`crate::issues::atty_check`), skip the redraw entirely and just print one
status block per poll, separated by blank lines. Keeps logs readable in CI.

Pseudocode:

```rust
let mut prev_lines: Option<usize> = None;
loop {
    let output = render_run_state(&run, &jobs, args.compact);
    let line_count = output.lines().count();
    if is_tty {
        if let Some(n) = prev_lines {
            eprint!("\x1b[{n}F\x1b[J");
        }
    }
    eprint!("{output}");
    prev_lines = Some(line_count);
    if is_terminal_status(&run) { break; }
    tokio::time::sleep(Duration::from_secs(args.interval)).await;
}
```

`render_run_state` returns a `String` so it's pure and testable. The terminal
control codes stay outside it.

### Status detection

A run is terminal when its `status` field is one of: `completed`, `success`,
`failure`, `cancelled`, `skipped`, `timed_out`, `action_required`. The Gitea
API conventionally uses `status = "completed"` with the actual outcome in
`conclusion`, but historical responses sometimes put the outcome in `status`
directly — keep the union to match the existing implementation's tolerance.

`is_failure_conclusion(c) = c ∈ {"failure", "cancelled", "timed_out", "action_required"}`.

## Components

- `WatchArgs` (struct) — clap-derived CLI args. As above.
- `pick_run(api, owner, repo) -> Result<i64>` — picker flow. Returns the
  chosen run ID. Bails on cancel with code 130.
- `render_run_state(run: &WorkflowRun, jobs: &[ActionWorkflowJob], compact: bool) -> String`
  — pure formatter. Status header + filtered job/step tree. Returns the full
  block including trailing newline. Easily unit-testable. Operates on the
  typed `ActionWorkflowJob` from the gitea-api crate (which carries
  `Vec<ActionWorkflowStep>`).
- `redraw(prev_lines: &mut Option<usize>, body: &str, is_tty: bool)` — handles
  the cursor-up/clear sequence; degrades to plain prints when non-TTY.
- `is_terminal_status(run) -> bool` and `is_failure_conclusion(s) -> bool` —
  small predicates, unit-testable.

`watch_run` becomes glue: arg resolution → picker if needed → polling loop
calling `render_run_state` and `redraw` → final exit code computation.

## Testing

Unit tests for the pure functions:

- `render_run_state` with: a passing job (with steps), a job with one failing step among passing steps, a queued job. Test `compact=true` vs `false` for each fixture; snapshot the output.
- `is_terminal_status` / `is_failure_conclusion` truth tables.
- Picker partitioning: takes a `Vec<WorkflowRun>` fixture, asserts in-progress
  vs recent split.

No integration tests against a live server — `cli.rs` already covers
`gt run --help` parses; that's enough.

## Out of scope / follow-ups

- Mid-run log streaming. (See gh-parity policy: `gh` doesn't do this; logs
  are `gh run view --log` territory.)
- `gt run view --log` and `--log-failed` flags — would close another gh gap
  but isn't needed for `watch` parity.
- Run filtering by branch / event in the picker. gh's picker only shows id +
  status + title; we should match.
