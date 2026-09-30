# {{intake_id}} — Breakdown

<!-- Tasks, order, dependencies and shared interfaces (workflow §5.5). Every
     REQ needs a task that owns it; every leaf task is a file
     tasks/TASK-xxx.md. Aim for 400 words above "## Detail". -->

## Summary

<!-- At most 150 words: how many tasks, in what order and why, what is
     risky, and what changed since the last revision. -->

## Tasks and dependencies

<!-- One row per task; one line of output each. The dependency graph is not
     drawn here: `zforge intake graph {{intake_id}}` generates it from the
     tasks' `depends_on`.

     | Task | Serves | Depends on | Output |
     |------|--------|------------|--------|
     | TASK-001 Status filter | REQ-001 | — | `list --status` | -->

## Integration verification

<!-- How the whole feature is verified on a tree holding every task's output.
     Put the commands in this section's first code block, one per line;
     zforge runs them in order:

     ```bash
     cargo test
     ```

     Without a code block, zforge runs `project.test_command`. -->

## Open questions

## Detail

<!-- Optional. Shared interfaces between tasks, existing tests that must
     change and which task's `tests_may_change` lists them. -->
