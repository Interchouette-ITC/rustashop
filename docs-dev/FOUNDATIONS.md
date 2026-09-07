# rustashop foundations

This document frames the **technical identity** of rustashop for a modern, Wasm-aware commerce kernel. It complements the shipped commerce checklist in the product README (`docs/README.md`). It names the axes the product grows into so architecture discussions stay durable.

## Product identity (short)

| Pillar            | Opinion                                                                                                                                                                                  |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Core**          | One Rust commerce kernel: catalog, cart, checkout, orders, money as integers, inventory, payments, webhooks                                                                              |
| Clients       | Angular **shop** or Leptos+rangular **shop** on one API; rangular → Leptos (web) + GPUI (native). See [UI-RENDERERS.md](UI-RENDERERS.md) |
| **AI native**     | Discovery, shopping agents, catalog assist, pricing/promos, support, MCP, and autonomous jobs are product surfaces on that API - not a side app ([AI-NATIVE.md](AI-NATIVE.md))           |
| **Live state**    | WebSocket (then optionally WebTransport) is first-class for shop and admin live updates; REST/OpenAPI for bootstrap, clear mutations, and inbound provider webhooks                      |
| **Extensibility** | Stable interfaces: OpenAPI for UIs; WIT / Component Model for plugins; optional sandboxed polyglot scripts for merchants, migrations, and agents                                         |
| **Persistence**   | A transactional store owned by the host kernel (**Postgres** via Docker compose + SQLx/SeaORM adapters). Analytics engines, embedded scratch databases, and GraphQL (if added) are **not** the system of record |
| **Surfaces**      | Domains and deploy tips in [DOMAINS.md](DOMAINS.md) (`interchouette.net` tip, `.ai` / `.io` / `.dev` / `.app`, geo redirects)                                                            |

GraphQL and columnar/analytics tools may appear later as **API or reporting choices**. They are independent product questions from “where do orders live.”

## HTTP stack (house pattern)

A **split stack**: a full **Actix-web** kernel for the product API, and a lighter **Axum** surface for MCP and agent tools.

| Surface             | Framework                                               | Owns                                                                 |
| ------------------- | ------------------------------------------------------- | -------------------------------------------------------------------- |
| **Commerce kernel** | **Actix-web** (+ OpenAPI via utoipa, WebSocket gateway) | Catalog, cart, checkout, orders, webhooks, admin REST, realtime push |
| **MCP / tools**     | **Axum**                                                | Streamable HTTP MCP, first-party agent tools, narrow ops endpoints   |
| **Runtime**         | **Tokio**                                               | Shared async runtime for both binaries/crates                        |

The MCP layer **reuses domain capabilities** from the kernel (HTTP internal calls and/or shared `domain` crates). It does not reimplement catalog, cart, or checkout.

Crate layout today: `domain`, `persist`, **`api`** (Actix), **`mcp`** (Axum), **`extensions`** (WIT), **`sandbox`** (Wasmer). Room remains for a dedicated `realtime` crate when that surface grows.

## Two contracts, one domain

| Contract                  | Audience                                 | Owns                                                                                               |
| ------------------------- | ---------------------------------------- | -------------------------------------------------------------------------------------------------- |
| **OpenAPI** (+ HTTP)      | Angular, rangular, external HTTP clients | Resources, auth stubs, idempotent checkout, webhooks ingress                                       |
| **WIT / Component Model** | Extension authors                        | Versioned hooks (pricing adjust, shipping quote, tax rule, …) with host-provided capabilities only |
| **Realtime events**       | Both UIs (and admin)                     | Typed push aligned with domain events (cart totals, stock, order status)                           |

Guests (Wasm components or sandboxed runtimes) never open the database. The host authorizes and commits.

## Three Wasm roles (keep them distinct)

rustashop is **Wasm-oriented** the way Meteor was **realtime-oriented**: an opinion about defaults, not a claim that every byte runs inside one engine.

| Role                   | Typical tech                                                                               | Job                                                                                          |
| ---------------------- | ------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| **Storefront UI Wasm** | rangular → Leptos in the browser                                                           | Client rendering and UX for UI option B                                                      |
| **Plugin Wasm**        | Component Model + WIT; host such as wasmtime (and/or Wasmer where it fits the ABI)         | Safe, versioned commerce extensions                                                          |
| **Sandbox Wasm**       | [Wasmer SDK](https://wasmer.io/posts/wasmer-local-sandboxes-for-ai-agents) local sandboxes | Untrusted or polyglot code: Python / Node / PHP packages, agent tools, demos, migration glue |

Optional: a small interpreter-style engine (for example wasmi) for tiny embedded evaluators. That is a tactical choice, not the architecture center. Serious Component Model hosting today centers on mature CM hosts; Wasmer’s WASIX / package story is especially relevant for **sandbox and polyglot** lanes.

Detail: [WASM-LAYERS.md](WASM-LAYERS.md), [EXTENSIONS.md](EXTENSIONS.md), [WASMER-SANDBOX.md](WASMER-SANDBOX.md).

## Realtime as a first-class axis

Meteor’s habit was: live sync is the default, not an afterthought. rustashop adopts the same _kind_ of opinion for commerce state that changes during a session.

- Push for cart/checkout session, inventory signals, order status, admin feeds.
- Both UI stacks subscribe to the **same** push channel.
- Server remains source of truth; optimistic UI may reconcile on events.
- Plugins emit or observe live effects only through **host-mediated** events.

Detail: [REALTIME.md](REALTIME.md).

## Capability axes (alongside commerce)

Catalog, cart, checkout, and orders ship over OpenAPI (and cart WebSocket). Related surfaces already in tree or documented:

1. **Realtime gateway** - cart WebSocket today; sandbox job events on the admin path; schema grows with more live feeds ([REALTIME.md](REALTIME.md)).
2. **Extension ABI** - WIT `pricing-adjust` host harness in `rustashop-extensions` ([EXTENSIONS.md](EXTENSIONS.md)).
3. **Sandbox lane** - Wasmer polyglot host + Angular `/sandbox` console ([WASMER-SANDBOX.md](WASMER-SANDBOX.md)).
4. **Polyglot / connectors** - PHP migration guest shipped; PyO3 reserved for trusted first-party connectors ([ADR 0003](adr/0003-pyo3-vs-wasmer-sandbox.md)).
5. **Module isolation tests** - guest loads that deny DB and assert capability boundaries (extend in CI as the ABI grows).
6. **AI-native tools** - backlog on the same domain ([AI-NATIVE.md](AI-NATIVE.md)); MCP crate is a workspace member without HTTP routes yet.
7. **Deploy surfaces** - `:dev` tip then `.ai` / `.io` / `.dev` / `.app` ([DOMAINS.md](DOMAINS.md)).

Crate layout (`domain`, `persist`, **`api`** on Actix, **`mcp`** on Axum, **`extensions`**, **`sandbox`**) matches these axes.

## Explicit non-goals for early foundations

- Making “everything runs in Wasmer” the definition of the commerce product.
- Syncing the entire catalog over WebSocket as a CRDT experiment.
- Replacing payment provider webhooks with WebSockets.
- Letting sandboxed guests capture cards or commit inventory alone.

## Related GitHub work

Track delivery under epics labeled `area:wasm`, `area:realtime`, `area:extensions`, and `area:ai`. HTTP stack decision: [#47](https://github.com/Interchouette-ITC/rustashop/issues/47). ADRs: [0001 Diesel](adr/0001-diesel-persistence.md), [0002 WIT plugin engine](adr/0002-wit-plugin-engine.md), [0003 PyO3 vs Wasmer](adr/0003-pyo3-vs-wasmer-sandbox.md).
