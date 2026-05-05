# T01 — Pack System

**Phase:** 1  
**Priority:** Critical (blocks T03)  
**Status:** TODO

## Goal

Add a pack system so `zf init --pack golang-tdd` scaffolds language-specific agents, skills, rules, hooks, and templates into `.zforge/`.

## Current State

`zf init` scaffolds a generic `.zforge/` directory. No pack concept exists.

## Required Changes

### 1. Pack directory structure (in repo)

```
packs/
└── golang-tdd/
    ├── pack.yaml          # metadata: name, version, language, description
    ├── agents/            # agent markdown files
    ├── skills/            # skill markdown files
    ├── rules/             # golang + tdd rules markdown files
    ├── hooks/             # pre/post hook scripts or descriptors
    └── templates/         # prompt templates (.tmpl)
```

### 2. CLI change

```
// src/main.rs — Commands::Init
Init {
    #[arg(long)]
    pack: Option<String>,   // add this field
    #[arg(long)]
    force: bool,
}
```

### 3. Config change

```yaml
# .zforge/config.yaml
project:
  name: myapp
  language: go
  test_command: go test ./...
  pack: golang-tdd         # new field
```

Add `pack` field to `ProjectConfig` in `src/config/mod.rs`.

### 4. Init logic (src/cli/init.rs)

- Accept `pack: Option<String>` param
- If pack provided: locate pack dir (embedded or from `~/.zforge/packs/`), copy into `.zforge/`
- If no pack: fall back to current generic scaffold

### 5. Pack resolution order

1. `./packs/<name>/` (repo-local, dev mode)
2. `~/.zforge/packs/<name>/` (user-installed)
3. Embedded in binary (use `include_dir` crate for distribution)

## Test Cases

- `zf init --pack golang-tdd` → `.zforge/agents/`, `.zforge/skills/`, `.zforge/rules/` populated
- `zf init --pack unknown-pack` → clear error "pack not found: unknown-pack"
- `zf init` (no pack) → existing behavior unchanged
- `zf init --pack golang-tdd --force` → overwrites existing `.zforge/`
- Config written includes `pack: golang-tdd`

## Files to Create / Modify

| File | Action |
|------|--------|
| `src/cli/init.rs` | Modify — accept pack param, copy pack files |
| `src/config/mod.rs` | Modify — add `pack` field to `ProjectConfig` |
| `packs/golang-tdd/pack.yaml` | Create |
| `packs/golang-tdd/` (full tree) | Create (skeleton, filled in T03) |
| `Cargo.toml` | Modify — add `include_dir` dep (optional) |
