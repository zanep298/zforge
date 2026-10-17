# Skill: intake-change

## Purpose

What to do when an accepted intake must change — a change request from a run, or a requirement the user wants to change after tasks were built. Read it with `zforge-intake`.

## Change requests

During a run, an agent that finds the contract must change writes
`changes/CHANGE-RUN-nnn.md` and the run stops `blocked`. Read it, explain it
to the user, and — if they agree — revise the contract files it concerns and
send them for review. Once accepted, the user hands over again; tasks whose
contract did not change are reused, not run again. To propose a change
yourself, `change_new` (MCP) creates the file with the required sections.

## Changing an accepted intake

Only what rests on a changed item is confirmed again and run again: a
task's requirements, their behavior rows, the decisions (`D-n`) it cites,
its breakdown row and the tasks it depends on; "Must not change" / "Out of
scope" bind everything; prose binds nothing. Keep stages in the template's
shape (REQ lines, the situations table, the decisions table, the tasks
table) — a stage that is not is compared whole.

- **Not merged yet** (`zforge status` does not say the handover is
  merged): edit the intake in place, send the changed files for review,
  confirm what readiness lists as resting on them, hand over again. Tasks
  whose items did not change are reused; a changed task continues from its
  earlier output, told what changed.
- **Already integrated or merged into the baseline**: `zforge intake amend
  <NEW> --from <ID>` (MCP `intake_amend`). Every file starts as a draft
  copied from the accepted revisions; change only what changes, rewrite
  `brief.md` around the change, `zforge intake review <NEW> --all`, show
  the pack, and ask the user to type `/accept <NEW> all`. Never accept for them.
- **A few contained fixes to an integrated intake** (a code review's
  findings, a small bug in what was built): `zforge intake amend <NEW>
  --from <ID> --fixup` (MCP `intake_amend`, `fixup: true`) instead of
  copying everything. 01-outcome is short (`profile: fixup`): the problem,
  new requirements numbered after the source's, what must not change;
  02-behavior and 03-solution say "As accepted in <ID> (revision n),
  except:" with one bullet per change (or `- No other change.`);
  04-breakdown has one task, `TASK-001`, and runs the project's test
  command; write `tasks/TASK-001.md` from its stub. All four stages are
  still reviewed, accepted and pinned. Use the full amend when the change
  reshapes behavior or needs several tasks.
