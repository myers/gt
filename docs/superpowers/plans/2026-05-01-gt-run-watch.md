# `gt run watch` gh-Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the existing `gt run watch` with a gh-parity version: optional run ID with picker fallback, step-level rendering, `--compact` step filter, opt-in `--exit-status`, scrollback-preserving redraw, and 3s default interval.

**Architecture:** Single-file refactor inside `gt/src/run.rs`. Replace `WatchArgs` and `watch_run`. Pure rendering and predicate functions become unit-testable. Glue (poll loop, picker, exit code) stays in `watch_run`. No new modules; no new files outside the test fixture data.

**Tech Stack:** Rust 2024 edition, `clap` derive, `inquire::Select` for the picker (already a dep), `tokio::time::sleep` for polling, the typed progenitor builders `get_workflow_run`, `get_workflow_runs`, `list_workflow_run_jobs` from `gitea-api`.

---

## Task 1: Replace `WatchArgs` with the gh-parity surface

**Files:**
- Modify: `gt/src/run.rs:49-57` (current `WatchArgs` struct)
- Modify: `gt/src/run.rs:200-265` (current `watch_run` body — gutted in this task)

This task lands the new CLI surface and a stub `watch_run` that compiles but doesn't yet do the work. Subsequent tasks fill in the behavior.

- [ ] **Step 1: Replace `WatchArgs` and stub the body**

Replace the existing `WatchArgs` struct (lines 49-57) and the entire `watch_run` function body (lines 200-265) with:

```rust
#[derive(Args)]
struct WatchArgs {
    /// Run ID. If omitted, prompt to pick from in-progress runs.
    id: Option<i64>,

    /// Refresh interval in seconds.
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

And the stub:

```rust
async fn watch_run(repo_args: &repo::RepoArgs, args: &WatchArgs) -> Result<()> {
    let _ = (repo_args, args);
    eyre::bail!("watch_run: not yet implemented")
}
```

- [ ] **Step 2: Run `cargo build -p gt` and `cargo test -p gt --bin gt`**

Run: `cargo build -p gt && cargo test -p gt --bin gt`
Expected: build succeeds, all existing tests pass. The `WatchArgs` change is type-compatible with the dispatch in `RunCommand::run` (line 75) because the function signature stayed the same.

- [ ] **Step 3: Verify `--help` shows the new flags**

Run: `cargo run -p gt -- run watch --help`
Expected: output includes `--interval`, `--compact`, `--exit-status`, and shows `id` as `[ID]` (optional). No mention of the old default of 5.

- [ ] **Step 4: Commit**

```bash
git add gt/src/run.rs
git commit -m "refactor(run): land gh-parity WatchArgs surface, stub body"
```

---

## Task 2: Status predicates with unit tests

**Files:**
- Modify: `gt/src/run.rs` — add free functions near the top of the file
- Test: `gt/src/run.rs` — `#[cfg(test)] mod tests` at the bottom

Two small, pure functions used by both the rendering and the loop control: `is_terminal_status` and `is_failure_conclusion`.

- [ ] **Step 1: Write the failing tests first**

Append to `gt/src/run.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_status_recognises_completed_and_outcome_aliases() {
        assert!(is_terminal_status("completed"));
        assert!(is_terminal_status("success"));
        assert!(is_terminal_status("failure"));
        assert!(is_terminal_status("cancelled"));
        assert!(is_terminal_status("skipped"));
        assert!(is_terminal_status("timed_out"));
        assert!(is_terminal_status("action_required"));
    }

    #[test]
    fn terminal_status_rejects_in_flight_states() {
        assert!(!is_terminal_status("in_progress"));
        assert!(!is_terminal_status("queued"));
        assert!(!is_terminal_status("waiting"));
        assert!(!is_terminal_status(""));
    }

    #[test]
    fn failure_conclusion_truth_table() {
        assert!(is_failure_conclusion("failure"));
        assert!(is_failure_conclusion("cancelled"));
        assert!(is_failure_conclusion("timed_out"));
        assert!(is_failure_conclusion("action_required"));

        assert!(!is_failure_conclusion("success"));
        assert!(!is_failure_conclusion("skipped"));
        assert!(!is_failure_conclusion(""));
        assert!(!is_failure_conclusion("in_progress"));
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail with "function not defined"**

Run: `cargo test -p gt --bin gt run::tests`
Expected: compile error, `cannot find function 'is_terminal_status'` (and `is_failure_conclusion`).

- [ ] **Step 3: Implement the predicates**

Add near the top of `gt/src/run.rs`, after the imports (above the existing `RunCommand` struct):

```rust
fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "success" | "failure" | "cancelled" | "skipped" | "timed_out" | "action_required",
    )
}

fn is_failure_conclusion(conclusion: &str) -> bool {
    matches!(
        conclusion,
        "failure" | "cancelled" | "timed_out" | "action_required",
    )
}
```

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cargo test -p gt --bin gt run::tests`
Expected: 3 passes.

- [ ] **Step 5: Commit**

```bash
git add gt/src/run.rs
git commit -m "feat(run): add is_terminal_status / is_failure_conclusion predicates"
```

---

## Task 3: `render_run_state` — non-compact rendering

**Files:**
- Modify: `gt/src/run.rs` — add `render_run_state` function near other helpers
- Test: `gt/src/run.rs` — extend `mod tests`

Pure formatter that takes a `&ActionWorkflowRun` and `&[ActionWorkflowJob]` and returns a multi-line `String`. This task implements the **non-compact** branch only.

- [ ] **Step 1: Write the failing test using a fixture builder**

Append to `mod tests`:

```rust
fn make_run(id: i64, title: &str, status: &str) -> gitea_api::types::ActionWorkflowRun {
    gitea_api::types::ActionWorkflowRun {
        id: Some(id),
        display_title: Some(title.to_string()),
        status: Some(status.to_string()),
        ..Default::default()
    }
}

fn make_step(name: &str, status: &str, conclusion: Option<&str>) -> gitea_api::types::ActionWorkflowStep {
    gitea_api::types::ActionWorkflowStep {
        name: Some(name.to_string()),
        status: Some(status.to_string()),
        conclusion: conclusion.map(str::to_string),
        ..Default::default()
    }
}

fn make_job(
    name: &str,
    status: &str,
    conclusion: Option<&str>,
    steps: Vec<gitea_api::types::ActionWorkflowStep>,
) -> gitea_api::types::ActionWorkflowJob {
    gitea_api::types::ActionWorkflowJob {
        name: Some(name.to_string()),
        status: Some(status.to_string()),
        conclusion: conclusion.map(str::to_string),
        steps,
        ..Default::default()
    }
}

#[test]
fn render_default_shows_every_step() {
    let run = make_run(42, "feat: hello", "in_progress");
    let jobs = vec![
        make_job(
            "build",
            "completed",
            Some("success"),
            vec![
                make_step("checkout", "completed", Some("success")),
                make_step("cargo test", "completed", Some("success")),
            ],
        ),
        make_job(
            "lint",
            "in_progress",
            None,
            vec![
                make_step("checkout", "completed", Some("success")),
                make_step("clippy", "in_progress", None),
            ],
        ),
    ];

    let out = render_run_state(&run, &jobs, false);

    assert!(out.contains("Run #42 — feat: hello"), "header missing: {out}");
    assert!(out.contains("Status: in_progress"), "status missing: {out}");
    assert!(out.contains("✓ build"));
    assert!(out.contains("✓ checkout"));
    assert!(out.contains("✓ cargo test"));
    assert!(out.contains("● lint"));
    assert!(out.contains("● clippy"));
}
```

- [ ] **Step 2: Run test to confirm it fails**

Run: `cargo test -p gt --bin gt run::tests::render_default_shows_every_step`
Expected: compile error, `cannot find function 'render_run_state'`.

- [ ] **Step 3: Implement `render_run_state` (non-compact branch only for now)**

Add to `gt/src/run.rs`, near the predicates from Task 2:

```rust
fn job_icon(status: &str, conclusion: &str) -> &'static str {
    match (status, conclusion) {
        (_, "success") => "✓",
        (_, "failure") => "✗",
        (_, "cancelled") => "⊘",
        (_, "timed_out") => "✗",
        (_, "action_required") => "✗",
        ("in_progress", _) => "●",
        ("queued", _) | ("waiting", _) => "○",
        _ => "?",
    }
}

fn render_run_state(
    run: &gitea_api::types::ActionWorkflowRun,
    jobs: &[gitea_api::types::ActionWorkflowJob],
    compact: bool,
) -> String {
    let _ = compact; // wired in Task 4
    let mut out = String::new();
    let id = run.id.unwrap_or(0);
    let title = run.display_title.as_deref().unwrap_or("(unnamed)");
    let status = run.status.as_deref().unwrap_or("unknown");
    let conclusion = run.conclusion.as_deref().unwrap_or("");

    out.push_str(&format!("Run #{id} — {title}\n"));
    if conclusion.is_empty() {
        out.push_str(&format!("Status: {status}\n"));
    } else {
        out.push_str(&format!("Status: {status} ({conclusion})\n"));
    }
    out.push('\n');

    for job in jobs {
        let jname = job.name.as_deref().unwrap_or("(unnamed job)");
        let jstatus = job.status.as_deref().unwrap_or("");
        let jconclusion = job.conclusion.as_deref().unwrap_or("");
        let icon = job_icon(jstatus, jconclusion);
        if jconclusion.is_empty() {
            out.push_str(&format!("  {icon} {jname} ({jstatus})\n"));
        } else if jconclusion == "success" {
            out.push_str(&format!("  {icon} {jname}\n"));
        } else {
            out.push_str(&format!("  {icon} {jname} ({jconclusion})\n"));
        }
        for step in &job.steps {
            let sname = step.name.as_deref().unwrap_or("(unnamed step)");
            let sstatus = step.status.as_deref().unwrap_or("");
            let sconclusion = step.conclusion.as_deref().unwrap_or("");
            let sicon = job_icon(sstatus, sconclusion);
            out.push_str(&format!("    {sicon} {sname}\n"));
        }
    }

    out
}
```

- [ ] **Step 4: Run test to confirm it passes**

Run: `cargo test -p gt --bin gt run::tests::render_default_shows_every_step`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add gt/src/run.rs
git commit -m "feat(run): render_run_state — header + step tree (non-compact)"
```

---

## Task 4: `render_run_state` — `--compact` filtering

**Files:**
- Modify: `gt/src/run.rs` — `render_run_state` body
- Test: `gt/src/run.rs` — extend `mod tests`

- [ ] **Step 1: Write failing tests for compact mode**

Append to `mod tests`:

```rust
#[test]
fn render_compact_hides_fully_successful_jobs() {
    let run = make_run(42, "feat: hello", "in_progress");
    let jobs = vec![
        make_job(
            "build",
            "completed",
            Some("success"),
            vec![make_step("cargo test", "completed", Some("success"))],
        ),
        make_job(
            "lint",
            "completed",
            Some("failure"),
            vec![
                make_step("checkout", "completed", Some("success")),
                make_step("clippy", "completed", Some("failure")),
            ],
        ),
    ];

    let out = render_run_state(&run, &jobs, true);

    // The all-green build job is hidden entirely.
    assert!(!out.contains("build"), "compact should hide successful job: {out}");
    // The failing lint job is shown.
    assert!(out.contains("✗ lint"), "lint job should appear: {out}");
    // Within the failing job, only the failed step shows.
    assert!(out.contains("✗ clippy"), "failed step should appear: {out}");
    assert!(!out.contains("✓ checkout"), "passing step should be hidden: {out}");
}

#[test]
fn render_compact_collapses_all_green() {
    let run = make_run(42, "all green", "in_progress");
    let jobs = vec![make_job(
        "build",
        "completed",
        Some("success"),
        vec![make_step("cargo test", "completed", Some("success"))],
    )];

    let out = render_run_state(&run, &jobs, true);

    assert!(out.contains("Run #42"));
    assert!(out.contains("(all steps passing so far)"), "fallback line missing: {out}");
    assert!(!out.contains("build"), "no jobs should appear in compact all-green: {out}");
}

#[test]
fn render_compact_shows_in_progress_job() {
    let run = make_run(42, "in flight", "in_progress");
    let jobs = vec![make_job(
        "build",
        "in_progress",
        None,
        vec![
            make_step("checkout", "completed", Some("success")),
            make_step("cargo test", "in_progress", None),
        ],
    )];

    let out = render_run_state(&run, &jobs, true);

    assert!(out.contains("● build"), "in_progress job missing: {out}");
    assert!(out.contains("● cargo test"), "in_progress step missing: {out}");
    assert!(!out.contains("✓ checkout"), "successful step should be hidden in compact: {out}");
}
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test -p gt --bin gt run::tests::render_compact`
Expected: 3 tests fail (the existing implementation ignores `compact`).

- [ ] **Step 3: Implement compact filtering**

Replace the `render_run_state` body so the inner job-loop respects `compact`:

```rust
fn render_run_state(
    run: &gitea_api::types::ActionWorkflowRun,
    jobs: &[gitea_api::types::ActionWorkflowJob],
    compact: bool,
) -> String {
    let mut out = String::new();
    let id = run.id.unwrap_or(0);
    let title = run.display_title.as_deref().unwrap_or("(unnamed)");
    let status = run.status.as_deref().unwrap_or("unknown");
    let conclusion = run.conclusion.as_deref().unwrap_or("");

    out.push_str(&format!("Run #{id} — {title}\n"));
    if conclusion.is_empty() {
        out.push_str(&format!("Status: {status}\n"));
    } else {
        out.push_str(&format!("Status: {status} ({conclusion})\n"));
    }
    out.push('\n');

    let mut any_job_rendered = false;
    for job in jobs {
        let jname = job.name.as_deref().unwrap_or("(unnamed job)");
        let jstatus = job.status.as_deref().unwrap_or("");
        let jconclusion = job.conclusion.as_deref().unwrap_or("");

        if compact && job_is_fully_green(job) {
            continue;
        }
        any_job_rendered = true;

        let icon = job_icon(jstatus, jconclusion);
        if jconclusion.is_empty() {
            out.push_str(&format!("  {icon} {jname} ({jstatus})\n"));
        } else if jconclusion == "success" {
            out.push_str(&format!("  {icon} {jname}\n"));
        } else {
            out.push_str(&format!("  {icon} {jname} ({jconclusion})\n"));
        }

        for step in &job.steps {
            if compact && step_is_hidden_in_compact(step) {
                continue;
            }
            let sname = step.name.as_deref().unwrap_or("(unnamed step)");
            let sstatus = step.status.as_deref().unwrap_or("");
            let sconclusion = step.conclusion.as_deref().unwrap_or("");
            let sicon = job_icon(sstatus, sconclusion);
            out.push_str(&format!("    {sicon} {sname}\n"));
        }
    }

    if compact && !any_job_rendered {
        out.push_str("  (all steps passing so far)\n");
    }

    out
}

fn job_is_fully_green(job: &gitea_api::types::ActionWorkflowJob) -> bool {
    let conclusion = job.conclusion.as_deref().unwrap_or("");
    if conclusion != "success" {
        return false;
    }
    job.steps
        .iter()
        .all(|s| s.conclusion.as_deref() == Some("success"))
}

fn step_is_hidden_in_compact(step: &gitea_api::types::ActionWorkflowStep) -> bool {
    let conclusion = step.conclusion.as_deref().unwrap_or("");
    let status = step.status.as_deref().unwrap_or("");
    let visible = matches!(conclusion, "failure" | "cancelled" | "timed_out" | "action_required")
        || matches!(status, "in_progress" | "queued" | "waiting");
    !visible
}
```

- [ ] **Step 4: Run tests to confirm pass**

Run: `cargo test -p gt --bin gt run::tests`
Expected: all `render_*` and predicate tests pass (5 tests so far in `mod tests`).

- [ ] **Step 5: Commit**

```bash
git add gt/src/run.rs
git commit -m "feat(run): --compact filter for render_run_state"
```

---

## Task 5: Picker — partition helper with tests

**Files:**
- Modify: `gt/src/run.rs` — add `partition_runs_for_picker` near the predicates
- Test: `gt/src/run.rs` — extend `mod tests`

The picker needs to split a run list into "in_progress" and "recent". Implement that as a pure helper before wiring up the inquire prompt.

- [ ] **Step 1: Write the failing test**

Append to `mod tests`:

```rust
#[test]
fn partition_runs_separates_in_progress_from_recent() {
    let runs = vec![
        make_run(1, "old success", "completed"),
        make_run(2, "queued", "queued"),
        make_run(3, "running", "in_progress"),
        make_run(4, "old failure", "failure"),
        make_run(5, "waiting", "waiting"),
    ];

    let (in_progress, recent) = partition_runs_for_picker(runs);

    let in_progress_ids: Vec<_> = in_progress.iter().map(|r| r.id.unwrap()).collect();
    let recent_ids: Vec<_> = recent.iter().map(|r| r.id.unwrap()).collect();

    assert_eq!(in_progress_ids, vec![2, 3, 5], "in-progress includes waiting/queued/in_progress");
    assert_eq!(recent_ids, vec![1, 4], "recent is everything else");
}

#[test]
fn partition_handles_missing_status_field() {
    // Status: None → treated as recent (not in-progress).
    let runs = vec![gitea_api::types::ActionWorkflowRun {
        id: Some(99),
        ..Default::default()
    }];

    let (in_progress, recent) = partition_runs_for_picker(runs);

    assert!(in_progress.is_empty());
    assert_eq!(recent.len(), 1);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test -p gt --bin gt run::tests::partition`
Expected: compile error, `cannot find function 'partition_runs_for_picker'`.

- [ ] **Step 3: Implement the partition helper**

Add to `gt/src/run.rs`:

```rust
fn partition_runs_for_picker(
    runs: Vec<gitea_api::types::ActionWorkflowRun>,
) -> (
    Vec<gitea_api::types::ActionWorkflowRun>,
    Vec<gitea_api::types::ActionWorkflowRun>,
) {
    let mut in_progress = Vec::new();
    let mut recent = Vec::new();
    for run in runs {
        let status = run.status.as_deref().unwrap_or("");
        if matches!(status, "in_progress" | "queued" | "waiting") {
            in_progress.push(run);
        } else {
            recent.push(run);
        }
    }
    (in_progress, recent)
}
```

- [ ] **Step 4: Run tests to confirm pass**

Run: `cargo test -p gt --bin gt run::tests::partition`
Expected: 2 PASS.

- [ ] **Step 5: Commit**

```bash
git add gt/src/run.rs
git commit -m "feat(run): partition_runs_for_picker helper for picker"
```

---

## Task 6: Picker UI — `pick_run` with `inquire::Select`

**Files:**
- Modify: `gt/src/run.rs` — add `pick_run` function

This task is the only one in the plan that performs I/O without unit tests, because `inquire::Select` reads from stdin / writes to stderr. The pure split helper from Task 5 is what we lean on for testability.

- [ ] **Step 1: Implement `pick_run`**

Add to `gt/src/run.rs`:

```rust
fn format_run_picker_label(run: &gitea_api::types::ActionWorkflowRun) -> String {
    let id = run.id.unwrap_or(0);
    let status = run.status.as_deref().unwrap_or("");
    let conclusion = run.conclusion.as_deref().unwrap_or("");
    let icon = job_icon(status, conclusion);
    let branch = run.head_branch.as_deref().unwrap_or("");
    let title = run.display_title.as_deref().unwrap_or("(unnamed)");
    format!("{icon} #{id} {branch} {title}")
}

async fn pick_run(api: &gitea_api::Gitea, owner: &str, repo: &str) -> Result<i64> {
    let resp = api
        .get_workflow_runs()
        .owner(owner)
        .repo(repo)
        .page(1)
        .limit(30)
        .send()
        .await
        .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
        .into_inner();

    let (in_progress, recent) = partition_runs_for_picker(resp.workflow_runs);

    if in_progress.is_empty() && recent.is_empty() {
        eprintln!("No workflow runs found");
        std::process::exit(0);
    }

    // Build labels and a parallel id vector. inquire::Select returns the
    // chosen label; we look up its id by index.
    let mut labels: Vec<String> = Vec::new();
    let mut ids: Vec<i64> = Vec::new();
    for run in in_progress.iter() {
        labels.push(format_run_picker_label(run));
        ids.push(run.id.unwrap_or(0));
    }
    if !in_progress.is_empty() && !recent.is_empty() {
        labels.push("───── recent ─────".to_string());
        ids.push(-1); // sentinel for the separator
    }
    for run in recent.iter().take(10) {
        labels.push(format_run_picker_label(run));
        ids.push(run.id.unwrap_or(0));
    }

    let chosen = match inquire::Select::new("Pick a run to watch:", labels.clone()).prompt() {
        Ok(s) => s,
        Err(inquire::InquireError::OperationCanceled)
        | Err(inquire::InquireError::OperationInterrupted) => {
            std::process::exit(130);
        }
        Err(e) => return Err(eyre::eyre!("{e}")),
    };

    let idx = labels
        .iter()
        .position(|l| l == &chosen)
        .ok_or_else(|| eyre::eyre!("internal: picked label not found"))?;
    let id = ids[idx];
    if id < 0 {
        // User somehow selected the separator; treat as cancel.
        std::process::exit(130);
    }
    Ok(id)
}
```

- [ ] **Step 2: Build and run existing tests**

Run: `cargo build -p gt && cargo test -p gt --bin gt`
Expected: build succeeds, all existing tests pass. No new tests yet — `pick_run` is interactive I/O.

- [ ] **Step 3: Commit**

```bash
git add gt/src/run.rs
git commit -m "feat(run): pick_run picker using inquire::Select"
```

---

## Task 7: Polling loop and final exit code

**Files:**
- Modify: `gt/src/run.rs` — replace stubbed `watch_run` body
- Add: helpers `redraw` and the use of `crate::issues::atty_check`

Wire everything together. The function resolves the run id (picker if missing), then runs the polling loop, then computes the exit code.

- [ ] **Step 1: Replace the stubbed `watch_run` body**

Replace the stub from Task 1 with:

```rust
async fn watch_run(repo_args: &repo::RepoArgs, args: &WatchArgs) -> Result<()> {
    let config = Config::load()?;
    let api = config.client()?;
    let repo_info = repo::resolve_repo(repo_args.repo.as_deref(), &config.url)?;
    let (owner, repo_name) = (repo_info.owner.as_str(), repo_info.name.as_str());

    let run_id = match args.id {
        Some(id) => id,
        None => pick_run(&api, owner, repo_name).await?,
    };

    let is_tty = crate::issues::atty_check();
    let mut prev_lines: Option<usize> = None;

    loop {
        let run = api
            .get_workflow_run()
            .owner(owner)
            .repo(repo_name)
            .run(run_id)
            .send()
            .await
            .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
            .into_inner();

        let jobs = api
            .list_workflow_run_jobs()
            .owner(owner)
            .repo(repo_name)
            .run(run_id)
            .send()
            .await
            .map_err(|e| eyre::eyre!("{}", gitea_api::GiteaError::from(e)))?
            .into_inner()
            .jobs;

        let body = render_run_state(&run, &jobs, args.compact);
        let line_count = body.lines().count();

        if is_tty {
            if let Some(n) = prev_lines {
                eprint!("\x1b[{n}F\x1b[J");
            }
        } else if prev_lines.is_some() {
            eprintln!();
        }
        eprint!("{body}");
        prev_lines = Some(line_count);

        let status = run.status.as_deref().unwrap_or("");
        if is_terminal_status(status) {
            let conclusion = run.conclusion.as_deref().unwrap_or("");
            if args.exit_status && (is_failure_conclusion(status) || is_failure_conclusion(conclusion)) {
                std::process::exit(1);
            }
            return Ok(());
        }

        tokio::time::sleep(std::time::Duration::from_secs(args.interval)).await;
    }
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p gt`
Expected: build succeeds.

- [ ] **Step 3: Run all existing tests**

Run: `cargo test -p gt`
Expected: all tests pass (the new code is glue; render/predicate tests still cover the testable surface).

- [ ] **Step 4: Smoke test against the live server**

Pick a recently-completed run id from `gt run list` and try:

Run: `cargo run -p gt -- run watch <id> --interval 3`
Expected: status block prints once, immediately reaches terminal state, exits 0.

Run: `cargo run -p gt -- run watch <id> --interval 3 --compact`
Expected: same but with successful steps hidden.

Run: `cargo run -p gt -- run watch <known-failed-id> --exit-status`
Expected: exits 1 (run conclusion is failure and `--exit-status` is set). Without `--exit-status`, same run exits 0.

Run: `cargo run -p gt -- run watch`
Expected: picker appears with in-progress runs first, then recent. Selecting a run watches it. Ctrl-C exits 130.

- [ ] **Step 5: Commit**

```bash
git add gt/src/run.rs
git commit -m "feat(run): wire watch_run polling loop, picker, exit code"
```

---

## Task 8: Bump version and update CLAUDE.md/README hooks

**Files:**
- Modify: `gt/Cargo.toml` — version bump
- Modify: `Cargo.lock` — auto-updated by cargo
- (Optional) `README.md` — README's `--help` block doesn't include subcommand-level help, so no change needed there.

This is a behavior-changing minor bump per the project's CLAUDE.md ("new behavior, anything the user could notice"). The `--exit-status` flag *is* a breaking change to the prior `gt run watch` exit-code contract, but per CLAUDE.md's stability section this is alpha and we don't preserve old behavior — so a minor bump is correct.

- [ ] **Step 1: Bump version**

In `gt/Cargo.toml`, change `version = "0.2.1"` to `version = "0.3.0"`.

- [ ] **Step 2: Refresh Cargo.lock**

Run: `cargo build -p gt`
Expected: Cargo.lock updates `gt v0.2.1 → v0.3.0`.

- [ ] **Step 3: Verify --version**

Run: `cargo run -p gt -- --version`
Expected: `gt 0.3.0 (<sha>-dirty, built <date>)` (dirty until commit).

- [ ] **Step 4: Commit**

```bash
git add gt/Cargo.toml Cargo.lock
git commit -m "chore: bump gt to 0.3.0 for run watch gh-parity"
```

---

## Self-Review

**Spec coverage check:**

| Spec section | Task |
|---|---|
| CLI surface (Option id, --interval=3, --compact, --exit-status) | Task 1 |
| Picker — fetch, partition, prompt, cancel-130 | Tasks 5 + 6 |
| Step-level rendering (default) | Task 3 |
| --compact rules | Task 4 |
| Exit codes (default 0, --exit-status escalates failure) | Task 7 |
| Scrollback-preserving redraw (cursor-up + clear) | Task 7 |
| TTY/non-TTY split | Task 7 |
| `is_terminal_status`, `is_failure_conclusion` predicates | Task 2 |
| Switch from `raw_get` to typed `list_workflow_run_jobs` | Task 7 |
| Tests: render w/ passing/failing/queued, predicates, partitioning | Tasks 2 + 3 + 4 + 5 |

No spec gaps. Mid-run log streaming and `gt run view --log` are explicit non-goals; not in the plan.

**Placeholder scan:** none. Every step has full code and exact commands.

**Type/name consistency:**
- `render_run_state(run: &ActionWorkflowRun, jobs: &[ActionWorkflowJob], compact: bool) -> String` used identically in Tasks 3, 4, 7.
- `is_terminal_status(&str) -> bool`, `is_failure_conclusion(&str) -> bool` defined Task 2, used Task 7.
- `partition_runs_for_picker(Vec<ActionWorkflowRun>) -> (Vec<...>, Vec<...>)` defined Task 5, used Task 6.
- `pick_run(&Gitea, &str, &str) -> Result<i64>` defined Task 6, used Task 7.
- `job_icon(status: &str, conclusion: &str) -> &'static str` defined Task 3, reused Tasks 4 + 6.
- `job_is_fully_green` and `step_is_hidden_in_compact` defined Task 4, only used inside `render_run_state`.

All types match. Plan is self-consistent.
