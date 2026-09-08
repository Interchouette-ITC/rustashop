# Three Wasm layers

rustashop uses WebAssembly in **more than one place**. Collapsing them into a single “we are Wasm” slogan hides different trust models, toolchains, and release cadences. This note keeps the layers separate on purpose.

## Layer map

```text
┌─────────────────────────────────────────────────────────────┐
│  Clients                                                     │
│  Angular (TS)          rangular / Leptos (UI Wasm in browser)│
│         │                         │                          │
│         └──────────┬──────────────┘                          │
│                    │ OpenAPI + Realtime push                   │
└────────────────────┼────────────────────────────────────────┘
                     ▼
┌─────────────────────────────────────────────────────────────┐
│  Rust commerce kernel (host)                                 │
│  Actix HTTP · WS gateway · domain · persist · capability bus │
│       │                         │                            │
│       │ WIT / CM                │ Wasmer sandbox API         │
│       ▼                         ▼                            │
│  Plugin components         Polyglot / agent sandboxes        │
│  (pricing, shipping, …)    (Python, Node, PHP, scratch DB)   │
└─────────────────────────────────────────────────────────────┘
                     ▲
┌─────────────────────────────────────────────────────────────┐
│  Axum MCP / agent tools (narrow HTTP, same domain)           │
└─────────────────────────────────────────────────────────────┘
```

## 1. Storefront UI Wasm

|                         |                                                                            |
| ----------------------- | -------------------------------------------------------------------------- |
| **What**                | UI option B: rangular templates compiled toward Leptos/wasm in the browser |
| **Trust**               | User’s browser; talks to our API like any client                           |
| **Not responsible for** | Plugin ABI, running merchant Python, hosting Postgres                      |

Angular (UI option A) is not Wasm; it is an equal client on the same OpenAPI and push contracts.

## 2. Plugin Wasm (Component Model)

|                  |                                                                 |
| ---------------- | --------------------------------------------------------------- |
| **What**         | Versioned guest modules implementing commerce hooks via WIT     |
| **Host**         | Rust kernel loads components; grants only declared capabilities |
| **Trust**        | Semi-trusted extension authors; still no raw DB                 |
| **Success look** | Sylius-like boundaries, without PHP class overrides             |

See [EXTENSIONS.md](EXTENSIONS.md).

## 3. Sandbox Wasm (Wasmer SDK and peers)

|           |                                                                                                                                                 |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| **What**  | Embed runtimes as packages inside the host or (for demos) the browser: Python, Node/Edge.js, PHP, optional embedded Postgres packages, and more |
| **Why**   | Untrusted or foreign code, agent tool execution, migration glue, density without Docker-per-script                                              |
| **Trust** | Hostile or unknown code; hard capability and I/O limits; audit everything                                                                       |

See [WASMER-SANDBOX.md](WASMER-SANDBOX.md) and the Wasmer announcement: [Local Sandboxes for AI Agents](https://wasmer.io/posts/wasmer-local-sandboxes-for-ai-agents).

## Engine notes (non-locking)

| Engine / stack                  | Sensible use                                                   |
| ------------------------------- | -------------------------------------------------------------- |
| **Leptos / wasm-bindgen**       | Storefront UI Wasm                                             |
| **wasmtime (CM)**               | Plugin hosting via Serenade `serenade-component-host` + product WIT |
| **Wasmer SDK / WASIX packages** | Sandbox via Serenade `serenade-sandbox` + product fixtures/ABIs     |
| **wasmi**                       | Tiny embedded interpreters if a hook needs a minimal evaluator |

We stay **Wasm-first** without requiring every guest to share one engine on day one. Interfaces (WIT, sandbox job API) matter more than a single `.wasm` monoculture.

## Design rule

When proposing a feature, name **which layer** it belongs to. A Leptos cart page, a WIT `pricing-adjust` component, and a Wasmer-run Python quote script are three different deliverables that may share domain types only through the host.
