# Global Store and Install

zforge ships all prompt templates, agent definitions, and skill bundles baked into
the binary. On first install, you populate a global store at `~/.zforge/` that
multiple projects can share. This avoids duplicating agent definitions in every
project.

## One-time machine setup

```bash
zforge install
```

Extracts embedded templates, agents, and skills to `~/.zforge/`:

```
~/.zforge/
├── agents/
│   ├── spec-agent.md
│   ├── testspec-agent.md
│   ├── plan-agent.md
│   ├── code-agent.md
│   └── review-agent.md
├── skills/
│   ├── rust-patterns.md
│   ├── golang-patterns.md
│   └── ...
└── templates/
    ├── spec.tmpl
    ├── testspec.tmpl
    └── ...
```

Run `zforge install` once per machine. Re-run after upgrading zforge to refresh the
global store with updated templates and agents.

```bash
zforge install --force   # overwrite existing files (including local edits)
```

> **Warning:** `--force` overwrites any manual edits you've made to `~/.zforge/`
> files. Keep a copy if you've customized agent definitions.

## Project init (shared mode, default)

```bash
zforge init
```

Default mode points `.zforge/config.yaml` at `~/.zforge/agents` and
`~/.zforge/skills`. All projects on the machine share the same agent
definitions — update once, all projects get it.

```
project/.zforge/config.yaml paths.agents  →  ~/.zforge/agents/
project/.zforge/config.yaml paths.skills  →  ~/.zforge/skills/
```

`.zforge/config.yaml`, `memory/`, and `tasks/` are always local to the project.

## Project init (local mode)

```bash
zforge init --local
```

Copies every template, agent definition, and skill bundle into the project
instead of pointing at the global store. Use this when:

- The project is a fork of zforge itself
- You need per-project customization of agent prompts
- You're sharing the repo with people who don't have the global store installed
- CI needs to run zforge without a pre-installed `~/.zforge/`

```
project/.zforge/agents/   ← full copies, not symlinks
project/.zforge/skills/   ← full copies
project/.zforge/templates/ ← full copies
```

Changes to local copies don't affect other projects.

## After upgrading zforge

```bash
cargo install --path . --force   # install new binary
zforge install                   # refresh ~/.zforge/ with updated templates/agents
```

Projects in shared mode pick up the updates immediately because their config
points at `~/.zforge/`.
Projects in local mode need a manual refresh:

```bash
cd my-project
zforge init --local --force      # overwrite local copies with new embedded versions
```

## Precedence: disk vs embedded

At runtime, `prompt::engine` looks for `.tmpl` files in this order:

1. Project-local `.zforge/agents/` (or whatever `agents_dir` resolves to)
2. Embedded binary fallback (always available, even without `~/.zforge/`)

This means even without running `zforge install`, prompts work. The embedded
fallback exists so CI and containerized environments don't need a setup step.

## Multi-agent init

`zforge init` scaffolds for Claude Code by default. Pass `--agent` to target
others:

```bash
zforge init --agent claude      # default: .mcp.json + CLAUDE.md + .claude/
zforge init --agent opencode    # .opencode/ + .mcp.json
zforge init --agent codex       # codex config + .mcp.json
zforge init --agent all         # all three
```

Language detection runs automatically. For a Rust project, relevant Rust skills
from `~/.zforge/skills/` are linked into `.zforge/skills/` and referenced in agent
prompts.

## Checking what's installed

```bash
ls ~/.zforge/agents/     # agent definitions
ls ~/.zforge/skills/     # skill bundles
zforge --version         # binary version
```
