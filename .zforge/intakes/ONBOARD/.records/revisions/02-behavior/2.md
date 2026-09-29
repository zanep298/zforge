# ONBOARD — Behavior

## Situations

Shared example: `backend-auth`, a Go service with a test command, a README
and a hand-written CLAUDE.md, never onboarded. Knowledge lives in
`docs/knowledge/`.

### Normal

1. **Not onboarded yet** (REQ-001). `zforge status` in backend-auth shows
   `project: not onboarded` above the intakes, and its `next` is
   `zforge onboard`. `zforge init` run in a terminal ends with
   `Onboard now? [Y/n]`; `init --force`, `migrate` and a non-interactive
   `init` never ask and never call a model.

2. **Probe** (REQ-002). `zforge onboard` first probes, without a model, at
   the current commit `c0`, and prints and records:
   ```
   commit     c0 (main, clean)
   language   go 1.23, go.mod; 412 files, 58k lines
   docs       README.md, CLAUDE.md (project's own), docs/adr/ (6)
   codegraph  indexed at c0
   baseline   go test ./...  → FAIL in 94s: TestTokenRotation, TestAuditExport
   ```
   A working tree with uncommitted changes is refused: the knowledge must be
   pinned to a commit.

3. **Draft** (REQ-003, REQ-004). The agent drafts `domain.md`,
   `conventions.md` and `rules.md`. Each statement is an item with a stable
   ID and its evidence:
   ```
   - DOM-004: A refresh token is single-use; using it twice revokes the
     whole session family. (internal/token/refresh.go:88, internal/token/refresh_test.go:141)
   - CONV-002: Errors cross package boundaries wrapped with %w and a
     lowercase context prefix. (internal/token/store.go:52, internal/audit/writer.go:37)
   - RULE-001: Public HTTP responses never change shape without a new route
     version. (api/v1/router.go:12, docs/adr/0004-versioning.md:9)
   ```
   What the code cannot show goes under *Open questions*:
   `- [ ] Is the 15-minute access token lifetime a product rule or a default? (config/defaults.go:21)`.
   The agent sends each file for review.

4. **Review and accept** (REQ-005). As for intake files: `zforge onboard
   review domain.md` records revision 1; the user runs `zforge onboard
   accept domain.md` or `zforge onboard revise domain.md --note …` at a
   terminal. `zforge status` shows each file's state (`draft`, `in review`,
   `accepted rev 2`, `needs revision`). The project counts as onboarded once
   all three files are accepted and the baseline is settled (situation 12).

5. **Intake on the knowledge** (REQ-007). Writing `03-solution.md` of a new
   intake, the agent reads the accepted knowledge and cites it: `Keeps
   RULE-001: the new endpoint is /v2/...`. Where a stage departs from an
   item, it says so in the stage, with the item's ID and why, for the user
   to accept or not; it never departs silently.

6. **Run with the knowledge** (REQ-008). A run's prompt has a *Project
   knowledge* section: every accepted `conventions.md` and `rules.md` item,
   and the `domain.md` items that concern the task. The run's trace records
   which knowledge revisions and item IDs it was given. A review finding can
   name one: `- internal/audit/writer.go: breaks CONV-002 (error not wrapped)`.

7. **Stale** (REQ-009). After commit `c5` changes the lines
   `internal/token/refresh.go:80-95`, `zforge status` shows
   `knowledge: 1 item stale — DOM-004 (domain.md)`. `zforge onboard refresh`
   redrafts DOM-004 only, against `c5`; `domain.md` goes to review as its
   next revision, the rest unchanged, and the diff shows exactly what moved.

8. **Readiness** (REQ-010). `zforge readiness FEAT` on a project that is
   not onboarded, or has stale items, or a red baseline not settled, lists
   each as a warning and still passes. With `onboarding.required: true` in
   the config, the same gaps fail readiness and `handover` refuses.

9. **Commitments** (REQ-011). `zforge knowledge index` writes the record of
   what intakes committed to — each REQ and binding decision with
   `verified` / `integration_verified` / `integrated` — as a generated file
   of its own next to the knowledge (not reviewed, marked generated). The
   three knowledge files are never rewritten by it.

### Edge

10. **Already onboarded**. `zforge onboard` on an onboarded project prints
    the state (files, revisions, stale items, baseline) and changes nothing;
    `zforge onboard refresh` is how knowledge changes.

11. **Existing docs disagree with the code**. README says tokens expire in
    30 minutes, the code says 15: the draft states what the code does, cites
    both, and asks under *Open questions* which one is intended.

12. **Red baseline** (REQ-002, REQ-010). The probe found TestTokenRotation
    and TestAuditExport failing at `c0`. The user either fixes them and
    probes again, or records them as known failures at a terminal
    (`zforge onboard baseline --known TestTokenRotation,TestAuditExport`).
    Readiness then notes the known failures instead of warning. Unknown new
    failures still warn.

12b. **A run on a known-red baseline** (REQ-010). With TestTokenRotation and
    TestAuditExport known, a run whose test command fails only on those two
    passes its verification; `verified` records the known failures it
    tolerated. A run that also fails TestLogin fails, naming TestLogin. If
    zforge cannot tell which tests failed from the output (no parser for the
    language, or the command died before running tests), the run fails as
    today — a known list never turns an unreadable result into a pass.

13. **Hand edit**. The user edits `docs/knowledge/rules.md` directly. It
    shows as `changed since review`; intakes and runs keep using the last
    accepted revision until the edit is reviewed and accepted.

14. **Fresh clone** (REQ-006). A teammate clones backend-auth after the
    knowledge was accepted and committed. On their machine `zforge status`
    shows the same files accepted at the same revisions, and their runs get
    the same knowledge.

15. **Moved lines** (REQ-009). A commit inserts ten lines above
    `refresh.go:88` without touching the cited code. DOM-004 is not stale;
    its evidence is shown at its new line.

16. **Large repository**. The draft covers the codebase module by module;
    `domain.md` has a section per module, and the agent reports which
    modules it covered. A module it did not reach is listed, not silently
    skipped.

17. **Not onboarded, work goes on**. A project that never onboards can still
    write intakes, pass readiness (with the warning) and run tasks; the run's
    prompt simply has no *Project knowledge* section.

### Errors

18. **Uncited or wrong evidence** (REQ-004). `zforge onboard review
    domain.md` refuses a statement without evidence, or evidence pointing to
    a file that does not exist at the pinned commit or a line past its end:
    `DOM-007: internal/token/rotate.go:200 — file has 143 lines`.

19. **Probe cannot run the tests**. No test command, or it fails to start or
    times out: the probe records that, the baseline counts as red, and
    onboarding continues.

20. **Refresh interrupted or out of budget**. Nothing already accepted is
    replaced; the stale items stay stale and `refresh` can be run again.

## Business rules

1. Knowledge describes the code as it is at its pinned commit, not as it
   should be. What should change goes through an intake.
2. A statement has evidence in the repository; anything else is an open
   question. A file with an unchecked open question can be reviewed but not
   accepted.
3. Item IDs (`DOM-`, `CONV-`, `RULE-` + number) are stable across revisions;
   a removed item's ID is not reused.
4. Only accepted revisions feed intakes and runs. Nothing — a run, the intake
   agent, `knowledge index` — changes a knowledge file without it going
   through review again.
5. An item is stale when the text of its cited lines differs between its
   pinned commit and HEAD, or a cited file is gone. Lines that only moved do
   not make it stale.
6. A task contract outranks the knowledge: a contract may depart from an
   item, but only by saying so, with the item's ID, in a file the user
   accepted.
7. Knowledge holds what is specific to this project; general advice stays in
   skills and rules.
8. A known baseline failure is tolerated by name only: a run passes when
   every failing test is on the known list and zforge could read which tests
   failed. Fixing a known failure removes it from the list at the next probe.

## Existing behavior to preserve

- `init`, `install`, `migrate` never call a model; `init --force` repeats
  safely.
- The intake flow, readiness and runs work unchanged for a project that is
  not onboarded, apart from readiness warnings.
- Accept, revise and handover need a terminal and a typed confirmation.
- `verified` / `integration_verified` / `integrated` keep their meaning and
  are derived from records and git only.
- A run's verdict still comes from the test command and protected tests; the
  knowledge informs the agent, it does not judge the run.

## Open questions

- [x] With a red baseline recorded as known, does a run still need the whole
  suite green? — No: it passes when every failure is a known one and the
  failing tests could be read (situation 12b, rule 8).
