# T07 — Remote MCP Runtime

**Phase:** 5  
**Priority:** Low (last phase)  
**Depends on:** T04  
**Status:** TODO

## Goal

Run a shared MCP server accessible over HTTP/SSE so teams share the same tool runtime.

## Architecture

```
developer machine
  └── AI agent (Claude Code / OpenCode)
        └── MCP client (HTTP) ──────→ Remote MCP server
                                           └── zf mcp-server tools
                                           └── auth middleware
                                           └── project context
```

## CLI Command

```
zf mcp-server --port 8080 --auth-token <TOKEN>
```

Current `zf mcp` runs stdio only. New `mcp-server` sub-command runs HTTP.

### src/main.rs addition

```rust
MpcServer {
    #[arg(long, default_value = "8080")]
    port: u16,
    #[arg(long, env = "ZFLOW_MCP_TOKEN")]
    auth_token: Option<String>,
},
```

## Transport

Use SSE (Server-Sent Events) for streaming tool responses, matching the MCP spec for HTTP transport.

Crates: `axum` + `tokio` (already likely in deps or add them).

## Auth

Bearer token auth. If `--auth-token` set:
- Require `Authorization: Bearer <TOKEN>` on all requests
- Return 401 on missing/wrong token

## Config

```yaml
mcp:
  remote:
    port: 8080
    auth_token_env: ZFLOW_MCP_TOKEN   # env var name (never store token in config)
```

Add `McpConfig` to `src/config/mod.rs`.

## Security Constraints

- Never log auth tokens
- Bind to `127.0.0.1` by default; require explicit `--bind 0.0.0.0` for external exposure
- HTTPS only in production (document TLS termination via reverse proxy)
- Rate limit: 60 requests/minute per IP

## Deployment Pattern

```
# Dockerfile for team deployment
FROM debian:bookworm-slim
COPY zf /usr/local/bin/zf
ENV ZFLOW_MCP_TOKEN=<set-at-runtime>
CMD ["zf", "mcp-server", "--port", "8080"]
```

## File Structure

```
src/mcp/
├── mod.rs          # stdio mode (existing)
├── server.rs       # HTTP/SSE mode (new)
├── auth.rs         # bearer token middleware (new)
└── tools/          # shared tool implementations (T04)
```

## Test Cases

- Server starts on configured port
- Request without token → 401
- Request with wrong token → 401
- Request with correct token → tool response
- `list_tools` returns same tools as stdio mode
- Bind to loopback by default (not 0.0.0.0)
