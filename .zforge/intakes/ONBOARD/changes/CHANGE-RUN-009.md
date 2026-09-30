---
task: TASK-005
run: RUN-009
---

## Contract in force

`.zforge/intakes/ONBOARD/tasks/TASK-005.md` (as handed over in
HANDOVER-004): Output — "`zforge knowledge index` writes
`<knowledge.dir>/commitments.md` (generated, not reviewed) and
`.zforge/knowledge/commitments.json`" — and AC-01, which pins the first
path to the literal default, `docs/knowledge/commitments.md`. Constraints —
"The whole existing suite passes, and no existing test file changes except
those in `tests_may_change`" — and `tests_may_change: [tests/intake_test.rs,
tests/run_test.rs]`.

## Evidence

Implemented exactly as specified: `src/knowledge/commitments.rs::write`
writes `commitments.md` under `config.knowledge_dir()` (default
`docs/knowledge/`, outside `.zforge/`) and `commitments.json` under
`.zforge/knowledge/`, wired `zforge knowledge index` and the post-decision
refresh (`intake accept`/`revise`, `handover`) to it, and updated
`tests/intake_test.rs` and `tests/run_test.rs` (both in `tests_may_change`)
to read the new locations. `cargo fmt --check` and
`cargo clippy --all-targets -- -D warnings` are clean.

`cargo test` then fails two tests in `tests/feature_run_test.rs`, which is
**not** in `tests_may_change` and which TASK-005 does not name as needing a
change (04-breakdown's "Existing tests that must change" note lists only
`tests/intake_test.rs` and `tests/run_test.rs`):

```
---- a_chain_runs_to_a_verified_feature ----
assertion `left == right` failed
  left: ("?? docs/\n", "main\n")
 right: ("", "main\n")

---- a_failed_integration_leaves_the_feature_unverified ----
assertion `left == right` failed
  left: ("?? docs/\n", "main\n")
 right: ("", "main\n")
```

Both tests snapshot `git status --porcelain` in the project's own checkout
(not a run's worktree) before calling `zforge run`, then again at the end —
after calling `entry(&p.knowledge(), "REQ-001")`, which runs
`zforge knowledge index` — and assert the two snapshots are byte-identical
(`tests/feature_run_test.rs:15-19` documents this as the file's own
invariant: "every scenario also checks that the user's checkout is left as
it was"). The test project's only `.gitignore` line is `.zforge/`
(`tests/support/run_project.rs:66`). Before this task, `knowledge index`
wrote only under `.zforge/knowledge/`, so the checkout stayed clean. Once
`commitments.md` writes to `docs/knowledge/` — outside `.zforge/`, and
correctly so per REQ-006 and the binding decision that `knowledge.dir`
content is committed with the code — `docs/` appears as a new untracked
directory the moment `knowledge index` runs, and the byte-identical
comparison fails. Four of the file's six signal tests don't call
`p.knowledge()` before their final comparison and still pass; only the two
that do are affected.

I looked for a way to satisfy AC-01's path and this invariant together
within the task's own scope and found none: the two are structurally in
tension once `commitments.md` moves out of the gitignored `.zforge/` tree,
which is what REQ-011 and the Output ask for. Moving `commitments.md` back
under `.zforge/knowledge/` would dodge the failure but contradicts the
Output and AC-01 as written (both name `<knowledge.dir>/commitments.md`,
i.e. `docs/knowledge/commitments.md` by default). Editing
`tests/feature_run_test.rs` would fix it but that file is outside
`tests_may_change`, and the task rules say not to work around a contract
constraint that way.

## Proposal

Add `tests/feature_run_test.rs` to TASK-005's `tests_may_change`, so the two
`checkout(&p)` comparisons in `a_chain_runs_to_a_verified_feature` and
`a_failed_integration_leaves_the_feature_unverified` can be updated to
account for the new tracked-but-uncommitted `docs/knowledge/commitments.md`
that `p.knowledge()` now produces as a side effect — e.g. asserting the
`git status` branch is unchanged and that no *previously tracked* path
changed, rather than byte-identical porcelain output; the run itself still
touches nothing in the checkout, which is what the file's stated invariant
(line 6) is protecting.

Alternatives considered and rejected:
- Write `commitments.md` under `.zforge/knowledge/` instead of
  `knowledge.dir`: contradicts the Output and AC-01 as accepted, and puts a
  file the user is meant to commit with the code (Binding decisions) in a
  directory the project's own `.gitignore` hides from git.
- Make `knowledge index` a no-op until the project is "onboarded": not in
  the contract, and breaks the existing (and still-required) behavior that
  `knowledge index` works on any intake project — including this task's own
  new assertion in `tests/intake_test.rs`, and the unconditional behavior
  the command has always had.

If nothing changes: TASK-005 stays undelivered, since I cannot make
`cargo test` pass without either breaking an untouchable test or
contradicting the Output/AC-01 the user accepted.

## Impact

- `tests/feature_run_test.rs`: two tests' final `checkout(&p)` comparison
  (lines ~70 and ~303 as currently written).
- No production code, interface, or other task's output is affected — this
  is a test-only gap the breakdown's "Existing tests that must change" list
  missed when TASK-005 was written.

## Decision needed

Accept adding `tests/feature_run_test.rs` to TASK-005's `tests_may_change`,
scoped to the two `checkout(&p)` assertions in
`a_chain_runs_to_a_verified_feature` and
`a_failed_integration_leaves_the_feature_unverified`, so they tolerate the
new untracked `docs/knowledge/commitments.md` `zforge knowledge index`
produces — or reject and say how `docs/knowledge/commitments.md` (Output,
AC-01) should instead coexist with that file's byte-identical-checkout
invariant.
