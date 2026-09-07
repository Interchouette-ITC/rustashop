# ADR 0003: PyO3 trusted connectors vs Wasmer untrusted scripts

- **Status:** Accepted
- **Date:** 2026-09-07
- **Issue:** [#41](https://github.com/Interchouette-ITC/rustashop/issues/41) (epic [#37](https://github.com/Interchouette-ITC/rustashop/issues/37))

## Context

Two ways exist to run Python next to the commerce kernel:

| Path | Mechanism | Typical author |
| --- | --- | --- |
| **Wasmer sandbox** | WASIX guest package (`rustashop-sandbox`) | Merchant, agent, migration glue |
| **Native connector** | In-process binding (for example **PyO3**) | rustashop / trusted partner |

They share a language surface but not a trust model. Contributors must not treat “upload a `.py` file” as the PyO3 path, and must not use PyO3 as the jail for untrusted code.

Agent tools (MCP / Axum) also need a default: execute untrusted or generated scripts in the sandbox unless the tool is explicitly first-party.

## Decision

1. **Wasmer** is the default execution lane for merchant scripts, agent-generated scripts, demos, and PHP/legacy migration guests. Guests propose values; the Rust host validates and commits.
2. **PyO3** (or similar native bindings) is reserved for **first-party connectors** we ship and maintain: shared types, low latency, no merchant upload of arbitrary source into that process.
3. **Agent tools default to sandbox** unless marked first-party in product docs and code. First-party tools call domain APIs directly; they do not become an open Python upload endpoint.
4. Do **not** expose PyO3 as the way operators upload arbitrary Python into the shop process.

## Consequences

| Do | Do not |
| --- | --- |
| Route untrusted / polyglot jobs through `rustashop-sandbox` | Treat Wasmer guests as able to commit money or inventory alone |
| Keep PyO3 work under explicit connector / integration issues | Wire merchant upload → in-process PyO3 |
| Document agent sandbox-by-default next to MCP surfaces | Collapse WIT plugins, Wasmer sandboxes, and PyO3 into one runtime |
| Revisit when the first official Python connector is chosen | Claim every Python path is sandboxed or every path is native |

## Related docs

- [WASMER-SANDBOX.md](../WASMER-SANDBOX.md)
- [EXTENSIONS.md](../EXTENSIONS.md) (native connectors section)
- WIT plugin engine remains separate: [ADR 0002](0002-wit-plugin-engine.md)
