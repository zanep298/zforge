# T04 — MCP Tools (9 Core Tools)

**Phase:** 3  
**Priority:** High  
**Depends on:** T01, T06  
**Status:** TODO

## Goal

Implement the 9 required MCP tools in `src/mcp/mod.rs` so agents can call them via the MCP protocol.

## Required Tools

| Tool | Description | Implementation |
|------|-------------|----------------|
| `search_code` | Ripgrep-based code search | spawn `rg` subprocess |
| `get_file` | Read file contents | `std::fs::read_to_string` |
| `get_function` | Extract function by name from file | parse with regex or tree-sitter |
| `run_go_test` | Run `go test` for specific test name | spawn `go test -run <name>` |
| `run_go_test_package` | Run `go test ./...` on package path | spawn `go test <pkg>` |
| `run_lint` | Run `golangci-lint run` | spawn subprocess |
| `get_git_diff` | Get git diff (staged or unstaged) | spawn `git diff` |
| `write_memory` | Write key to `.zforge/memory/` | `fs::write` |
| `read_memory` | Read key from `.zforge/memory/` | `fs::read_to_string` |

## MCP Server Architecture

Current `src/mcp/mod.rs` is a stub. Build on the JSON-RPC stdio protocol:

```
stdin  →  JSON-RPC request  →  dispatch to tool handler  →  JSON-RPC response  →  stdout
```

### Tool handler interface

```rust
pub trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> serde_json::Value;  // JSON Schema for params
    fn call(&self, params: serde_json::Value, ctx: &Context) -> Result<serde_json::Value>;
}
```

### Context

```rust
pub struct Context {
    pub project_root: PathBuf,
    pub memory_dir: PathBuf,
    pub tasks_dir: PathBuf,
}
```

## Tool Specs

### search_code

```json
{
  "pattern": "string (regex)",
  "path": "string (optional, default: project root)",
  "file_glob": "string (optional, e.g. '*.go')"
}
```

Returns: array of `{ file, line, text }` matches.

### get_file

```json
{ "path": "string" }
```

Returns: `{ content: string, lines: number }`.

### get_function

```json
{ "file": "string", "function": "string" }
```

Returns: `{ content: string, start_line: number, end_line: number }`.
Implementation: scan for `func <name>` or `func (.*) <name>` pattern.

### run_go_test

```json
{ "name": "string", "package": "string (optional)" }
```

Returns: `{ passed: bool, output: string, duration_ms: number }`.

### run_go_test_package

```json
{ "package": "string (default: './...')" }
```

Returns: `{ passed: bool, output: string, coverage: string }`.

### run_lint

```json
{ "path": "string (default: '.')" }
```

Returns: `{ passed: bool, issues: [{ file, line, message }] }`.

### get_git_diff

```json
{ "staged": "bool (default: false)", "file": "string (optional)" }
```

Returns: `{ diff: string }`.

### write_memory

```json
{ "key": "string", "content": "string" }
```

Writes to `.zforge/memory/<key>.md`. Returns `{ ok: true }`.

### read_memory

```json
{ "key": "string" }
```

Reads `.zforge/memory/<key>.md`. Returns `{ content: string }` or error.

## MCP Registration

Tools register via `list_tools` response so agents can discover them:

```rust
fn list_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(SearchCode),
        Box::new(GetFile),
        Box::new(GetFunction),
        Box::new(RunGoTest),
        Box::new(RunGoTestPackage),
        Box::new(RunLint),
        Box::new(GetGitDiff),
        Box::new(WriteMemory),
        Box::new(ReadMemory),
    ]
}
```

## File Structure

```
src/mcp/
├── mod.rs          # server loop (already exists — needs real impl)
├── tools/
│   ├── mod.rs      # Tool trait + registry
│   ├── search.rs   # search_code
│   ├── files.rs    # get_file, get_function
│   ├── testing.rs  # run_go_test, run_go_test_package
│   ├── lint.rs     # run_lint
│   ├── git.rs      # get_git_diff
│   └── memory.rs   # write_memory, read_memory
└── context.rs      # Context struct
```

## Test Cases

- `search_code` with pattern returns matching lines
- `get_file` returns content of existing file
- `get_file` returns error for missing file
- `get_function` finds `func Foo(` in file
- `run_go_test` returns `passed: true` for passing test
- `write_memory` + `read_memory` roundtrip
- MCP `list_tools` returns all 9 tools
- MCP JSON-RPC request/response format validates
