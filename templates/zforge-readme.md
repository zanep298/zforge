# .zforge — {{project_name}}

zforge's working directory for this project.

| Path | What it holds | Edit it? |
|------|---------------|----------|
| `config.yaml` | Test command, runner, execution policy (`budget_usd`, `max_iterations`, `review`), knowledge baseline | yes |
| `models.yaml` | Which model runs each phase (`code`, `review`); `zforge models` shows and sets it | yes |
| `intakes/<ID>/` | An intake: `01-outcome.md` … `04-breakdown.md`, `tasks/`, `changes/` | yes — then send for review |
| `intakes/<ID>/.records/` | Revisions, decisions, handover manifests | no — written by zforge |
| `runs/RUN-nnn/` | A run: `run.yaml`, `events.jsonl` (the source of truth), `progress.md`, `result.md` | no |
| `worktrees/<RUN>/` | A run's worktree, on branch `zforge/<task>/<run>` | no — `zforge run clean <RUN>` |
| `knowledge/` | `index.md` / `index.json`: each requirement, decided and built | no — `zforge knowledge index` |

## Workflow

```
zforge intake new FEAT                 →  write the four stage files and tasks/
zforge intake review FEAT <file>       →  send a file for review
zforge intake accept FEAT <file>          ← the user, in a terminal
zforge readiness FEAT                  →  can it be handed over?
zforge handover FEAT                      ← the user, in a terminal
zforge run HANDOVER-001                →  every task: code → {{test_command}} → sealed output;
                                          then the integration check
zforge status                          →  where each intake stands and what to do next
```

Skills are checklists in `{{skills_dir}}/` (for example
`{{skills_dir}}/review-patch.md`); agent definitions sit in the `agents/`
directory next to it. `zforge init --force` refreshes the copies each client
reads.
