# ONBOARD — Breakdown

## Phases

1. **Foundation** — a reviewed document set for knowledge, its items and
   evidence, the probe and baseline, commitments moved next to the
   knowledge: TASK-001, TASK-002, TASK-003, TASK-005.
2. **Freshness and gating** — stale items, handover pins and readiness:
   TASK-004, TASK-006.
3. **Use** — the run gets the knowledge and tolerates known failures; the
   intake side reads and cites it: TASK-007, TASK-009.
4. **Drafting** — the interactive skill and the headless draft and refresh:
   TASK-008.

## Tasks and dependencies

| Task | Serves | Depends on | Output |
|---|---|---|---|
| TASK-001 Document sets and knowledge review | REQ-005, REQ-006 | — | `DocSet` shared by intakes and knowledge; `zforge onboard review\|accept\|revise\|status`; `knowledge.dir`; `.gitattributes` union merge for `decisions.jsonl` |
| TASK-002 Items, evidence and lint | REQ-004 | TASK-001 | Item/ID/evidence parser; lint of evidence at `pinned`, ID stability, `covers`; run by `onboard review` |
| TASK-003 Probe, baseline and onboarding state | REQ-001, REQ-002, REQ-010 | TASK-001 | `zforge onboard` probe → `baseline.md`; `onboard baseline --known\|--clear` (terminal); "onboarded" state in `zforge status` and `next`; `init` offers onboarding in a terminal |
| TASK-004 Stale items | REQ-009 | TASK-002 | Text comparison pinned vs HEAD, moved detection; stale items in `onboard status` and `zforge status` |
| TASK-005 Commitments next to the knowledge | REQ-011 | TASK-001 | `knowledge index` writes `commitments.md` in `knowledge.dir`, `commitments.json` in `.zforge/knowledge/` |
| TASK-006 Handover pins and readiness | REQ-010 | TASK-002, TASK-003, TASK-004 | Manifest pins accepted knowledge revisions and the known-failure list; readiness project checks; `onboarding.required` |
| TASK-007 Knowledge and known failures in runs | REQ-008, REQ-010 | TASK-006 | `select.rs`; `{{project_knowledge}}` in both run prompts; trace records it; verification tolerates pinned known failures, records `tolerated` and `known_passing` |
| TASK-008 Drafting | REQ-003, REQ-009 | TASK-002, TASK-003, TASK-004 | `zforge-onboard` skill (interactive, module by module); `onboard draft` and `onboard refresh` headless with a budget, sending results for review |
| TASK-009 Intake side and MCP | REQ-007, REQ-005 | TASK-002, TASK-003 | Intake skill reads accepted knowledge and cites IDs; intake lint warns on unknown knowledge IDs; MCP `onboard_status`, `onboard_probe`, `onboard_review`; accept, revise and baseline added to `v15::FORBIDDEN` |

Order in the manifest: TASK-001, TASK-002, TASK-003, TASK-005, TASK-004,
TASK-006, TASK-007, TASK-008, TASK-009.

Existing tests that must change: TASK-005 moves the commitments output, which
`tests/intake_test.rs` (`.zforge/knowledge/index.md`, line 422) and
`tests/run_test.rs` (lines 636 and 1387) read; its contract lists them in
`tests_may_change`. No other task may change an existing test file.

## Shared interfaces

- **`DocSet`** (TASK-001): `open(root, kind, id)`, `files()`, `file(rel)`,
  `records_dir()`; `review`, `accept`, `revise`, `statuses`, `file_status`,
  `pending_review` and snapshot reads take a `DocSet`. `Intake` is a
  `DocSet` of kind intake at `.zforge/intakes/<ID>/`; knowledge is a
  `DocSet` of kind knowledge at `knowledge.dir`, records in
  `knowledge.dir/.records/`. The intake CLI and MCP behave exactly as now.
- **`Item`** (TASK-002): `{ id, file, section, text, evidence: Vec<Cite> }`,
  `Cite { path, start, end }`; `items(text) -> Vec<Item>`;
  `pinned(text) -> Option<commit>`; `covers(text) -> Vec<module>`.
- **Onboarding state** (TASK-003): `onboard::state(root) -> OnboardState {
  files: per-file DocStatus, baseline: { commit, result, failing, known },
  onboarded: bool }`, read by status, readiness and MCP.
- **Stale** (TASK-004): `stale::check(root) -> Vec<StaleItem { id, file,
  cite, reason: changed | gone }>` plus moved citations.
- **Manifest** (TASK-006): `knowledge: [{ file, revision, sha256 }]` and
  `known_failures: [test]`, both optional so manifests written before stay
  valid.
- **Selection** (TASK-007): `select::for_task(handover, task, limit) ->
  { text, items: Vec<id>, omitted: Vec<id>, snapshots: Vec<path> }`.
- **Config**: `knowledge.dir` (TASK-001), `knowledge.prompt_limit`
  (TASK-007), `knowledge.draft_budget_usd` (TASK-008),
  `onboarding.required` (TASK-006).

## Integration verification

Run from the repository root on the tree that holds every task's output:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The suite must also cover, end to end through the binary with a stub agent:
a project onboarded (three files accepted, baseline recorded) whose handover
pins the knowledge; a run whose prompt carries the rules and conventions; a
run that passes on known failures only; a cited line changed after
onboarding shows up stale in `zforge status`.

## Open questions

None.
