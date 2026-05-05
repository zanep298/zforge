# zforge Implementation Plan

Spec: `specs/refactors.md`
Language: Rust CLI (zf) — the TOOL itself; targets Golang projects

## Status

| ID | Title | Phase | Status |
|----|-------|-------|--------|
| T01 | Pack system + init --pack flag | 1 | TODO |
| T02 | CLI command aliases (implement / test) | 1 | TODO |
| T03 | golang-tdd pack: agents + skills + rules | 2 | TODO |
| T04 | MCP tools (9 tools) | 3 | TODO |
| T05 | Hooks system (pre + post) | 3 | TODO |
| T06 | Memory system (.zforge/memory/) | 2 | TODO |
| T07 | Remote MCP runtime | 5 | TODO |

## Phases

| Phase | Goal |
|-------|------|
| 1 | CLI + pack system |
| 2 | TDD workflow completeness |
| 3 | MCP tools |
| 4 | Multi-agent support |
| 5 | Remote MCP |

## Dependency Order

T01 → T02 → T03 → T06 → T04 → T05 → T07
