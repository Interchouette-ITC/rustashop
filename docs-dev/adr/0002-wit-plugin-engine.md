# ADR 0002: WIT plugin host engine

- **Status:** Accepted (wasmtime)
- **Date:** 2026-09-07
- **Issue:** [#34](https://github.com/Interchouette-ITC/rustashop/issues/34)

## Context

Extension plugins use the Wasm Component Model and WIT (`pricing-adjust` world v0). The host must load a guest component, call exported hooks, and keep undeclared imports denied. Two engines were candidates:

| Engine | Fit for WIT plugins |
| --- | --- |
| **wasmtime** | First-class Component Model + `bindgen!`; already used in `rustashop-extensions` |
| **Wasmer** | Strong for polyglot sandboxes / packages; Component Model support is a separate lane |

Wasmer remains the planned host for merchant/agent sandboxes ([#37](https://github.com/Interchouette-ITC/rustashop/issues/37)), which is a different trust and ABI surface than versioned WIT plugins.

## Decision

**Adopt wasmtime** as the WIT plugin host for rustashop commerce extensions.

Do **not** run the same guest through Wasmer for the plugin lane unless a later ADR revisits shared packaging.

## Consequences

| Do | Do not |
| --- | --- |
| Keep `rustashop-extensions` on wasmtime Component Model APIs | Block plugin work on Wasmer CM parity |
| Document Wasmer under the sandbox epic (#37) | Collapse plugin and sandbox engines into one requirement |
| Revisit only if product packaging forces a single engine | Rewrite persistence or HTTP as components |

## Evidence

- Fixture + host invoke: [#35](https://github.com/Interchouette-ITC/rustashop/issues/35) / PR #159
- Isolation + golden I/O: [#36](https://github.com/Interchouette-ITC/rustashop/issues/36) / PR #160
