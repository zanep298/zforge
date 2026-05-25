# PR1-08 — `zforge status --global`

## Goal

Aggregate the active task list across every project in the registry into a single dashboard view.

## Files touched

- `src/cli/status.rs` — extend with `--global` flag and `run_global` function

## Implementation

### Flag

Add to the existing status clap args:

```rust
#[arg(long)]
pub global: bool,

#[arg(long, default_value_t = 2000)]
pub timeout_ms: u64,

#[arg(long)]
pub json: bool,
```

### Dispatch

```rust
pub fn run(args: StatusArgs) -> Result<()> {
    if args.global {
        return run_global(args.timeout_ms, args.json);
    }
    run_local(args)
}
```

### `run_global`

```rust
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub fn run_global(timeout_ms: u64, json: bool) -> Result<()> {
    let registry = crate::registry::io::load()?;
    let mut rows: Vec<GlobalRow> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for entry in &registry.projects {
        let path = entry.path.clone();
        let name = entry.name.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = scan_project_tasks(&path);
            let _ = tx.send(result);
        });
        match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
            Ok(Ok(tasks)) => {
                for t in tasks {
                    rows.push(GlobalRow {
                        project: name.clone(),
                        task_id: t.id,
                        state: t.state,
                        flow: t.flow,
                        active_agent: t.active_agent,
                    });
                }
            }
            Ok(Err(e)) => {
                warnings.push(format!("skip {name}: {e}"));
            }
            Err(_) => {
                warnings.push(format!("skip {name}: timeout after {timeout_ms}ms"));
            }
        }
    }

    for w in &warnings {
        eprintln!("warning: {w}");
    }

    if json {
        let out = serde_json::to_string_pretty(&rows)?;
        println!("{out}");
    } else {
        print_table(&rows);
    }
    Ok(())
}
```

### `scan_project_tasks`

```rust
fn scan_project_tasks(project_path: &std::path::Path) -> Result<Vec<TaskSummary>> {
    let tasks_dir = project_path.join(".zforge/tasks");
    if !tasks_dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&tasks_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let state_file = entry.path().join(".state.yaml");
        if !state_file.exists() {
            continue;
        }
        let raw = match std::fs::read_to_string(&state_file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed: crate::state::TaskState = match serde_yaml::from_str(&raw) {
            Ok(p) => p,
            Err(_) => continue,
        };
        // Only include active tasks — anything below the terminal state in the task's flow.
        if !parsed.is_terminal_in_flow() {
            out.push(TaskSummary::from(parsed, entry.file_name().to_string_lossy().into_owned()));
        }
    }
    Ok(out)
}
```

### Types

```rust
#[derive(serde::Serialize)]
struct GlobalRow {
    project: String,
    task_id: String,
    state: String,
    flow: String,
    active_agent: Option<String>,
}
```

If `TaskState::is_terminal_in_flow` does not yet exist, add it as a small helper in `state/mod.rs` (compares `self.state` to the last state in `self.flow`).

## Acceptance criteria

- [ ] `zforge status --global` exits 0 with no projects registered (empty table).
- [ ] With three registered projects, each containing one active task, rows from all three appear in the output.
- [ ] A project whose `.state.yaml` is malformed YAML is skipped; warning goes to stderr; other projects still appear.
- [ ] A project whose path no longer exists is skipped; warning to stderr.
- [ ] `--timeout-ms 1` causes slow projects to be skipped with a timeout warning; fast ones still appear.
- [ ] `--json` outputs an array parseable by `jq '.[].project'`.
- [ ] Terminal-state tasks (already `Reviewed`) do not appear in the output.

## Depends on

- PR1-03.

## Notes

- Serial scan with per-project timeout — simpler than rayon. Revisit if benchmarks show pain at >50 projects.
- `recv_timeout` is from `std::sync::mpsc`; no tokio required.
- Spawned threads outlive the timeout — they finish on their own and the result is discarded. Acceptable for a one-shot CLI invocation.

---

```yaml
status: done
owner: ""
pr: ""
```
