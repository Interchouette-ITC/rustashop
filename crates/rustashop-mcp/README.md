# rustashop-mcp

Rust **rmcp** server for commerce tools. Shares the `TOOLS` catalog with in-app agents and proxies calls to the Actix commerce API (`rustashop-api`).

## Transports

| Mode | How | Use |
| --- | --- | --- |
| **stdio (host)** | `make run-mcp` | MCP over stdin/stdout |
| **HTTP (host)** | `make run-mcp-http` | Streamable HTTP on **8090** → `http://127.0.0.1:8090/mcp` |

```bash
# Commerce API must be up (default http://127.0.0.1:8080)
make run-api

make run-mcp          # stdio
make run-mcp-http     # Streamable HTTP
```

```bash
cargo run -p rustashop-mcp --bin rustashop-mcp
cargo run -p rustashop-mcp --bin rustashop-mcp -- --http
cargo run -p rustashop-mcp --bin rustashop-mcp -- --http --listen 127.0.0.1:8090
MCP_HTTP=true cargo run -p rustashop-mcp --bin rustashop-mcp
```

## Env

| Var | Default | Role |
| --- | --- | --- |
| `RUSTASHOP_API_BASE` | `http://127.0.0.1:8080` | Commerce API base for tool proxies |
| `MCP_HTTP` | unset | Force HTTP transport |
| `RUSTASHOP_MCP_ADDR` | `127.0.0.1:8090` | HTTP listen when `--http` / `MCP_HTTP` |
| `RUSTASHOP_MCP_ALLOW_COMMIT` | unset | Set to `1` / `true` to allow `place_order` / `patch_order_status` |
| `RUSTASHOP_ADMIN_API_TOKEN` | unset | Bearer for admin tools |
| `RUSTASHOP_ADMIN_API_PREFIX` | `admin` | Opaque admin URI segment |

HTTP routes: `/mcp` and `/mcp/` only (Streamable MCP).

Tool names and effects: [docs-dev/AI-TOOLS.md](../../docs-dev/AI-TOOLS.md).
