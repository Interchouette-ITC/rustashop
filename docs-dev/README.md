# Developer foundations (`docs-dev`)

Internal orientation for rustashop: **product and technical identity**, and the axes that sit next to the commerce surface documented in the [README](../docs/README.md) (catalog, cart, checkout, orders, both UIs, OpenAPI, compose).

Public contributor docs (`docs/ARCHITECTURE.md`, `docs/CONTRIBUTING.md`, …) live under `docs/`. **`docs-dev` is the living foundation set** so architecture debates are not trapped in chat.

## Documents

| Doc                                    | Topic                                                                                    |
| -------------------------------------- | ---------------------------------------------------------------------------------------- |
| [FOUNDATIONS.md](FOUNDATIONS.md)       | Overall identity: contracts, three Wasm roles, realtime default, AI-native, roadmap axes |
| [PERSISTENCE.md](PERSISTENCE.md)       | Parameterized queries, persist-param NUL hygiene, raw SQL gate                           |
| [AI-NATIVE.md](AI-NATIVE.md)           | AI across API and UIs: discovery, agents, catalog, pricing, support, MCP                 |
| [AI-TOOLS.md](AI-TOOLS.md)             | v0 commerce tool / function schema (MCP + in-app agents)                                 |
| [DOMAINS.md](DOMAINS.md)               | Hostnames (`ai` / `io` / `dev` / `app` / redirects) and deploy surfaces                  |
| [UI-RENDERERS.md](UI-RENDERERS.md)     | Angular + rangular dual track; Leptos web vs GPUI native hosts                           |
| [WASM-LAYERS.md](WASM-LAYERS.md)       | Storefront Wasm vs plugin Component Model vs sandbox runtimes                            |
| [REALTIME.md](REALTIME.md)             | WebSocket-first live shop state (Meteor-like opinion, rustashop protocol)                |
| [EXTENSIONS.md](EXTENSIONS.md)         | WIT plugin ABI, host capabilities, OpenAPI vs WIT                                        |
| [WASMER-SANDBOX.md](WASMER-SANDBOX.md) | Wasmer SDK: polyglot guests, agents, PHP legacy, playgrounds, connectors                 |

## How this relates to the shipped commerce surface

The README surface (catalog, cart, checkout, orders, both UIs, OpenAPI, compose) is the **current proof**. Foundations here say what we **design toward** so crate and API choices leave room for realtime, extensions, sandbox, and AI tools without becoming a pure REST monolith with plugins bolted on.

HTTP: **Actix-web** for the commerce kernel; **Axum** for MCP and agent tool surfaces ([FOUNDATIONS.md](FOUNDATIONS.md)).

## Issues

GitHub epics (created with this foundation set):

| Epic                                                            | Focus                                                    |
| --------------------------------------------------------------- | -------------------------------------------------------- |
| [#47](https://github.com/Interchouette-ITC/rustashop/issues/47) | HTTP stack: Actix kernel + Axum MCP                      |
| [#31](https://github.com/Interchouette-ITC/rustashop/issues/31) | Realtime WebSocket-first live state                      |
| [#34](https://github.com/Interchouette-ITC/rustashop/issues/34) | WIT Component Model extension ABI                        |
| [#37](https://github.com/Interchouette-ITC/rustashop/issues/37) | Wasmer polyglot sandbox and agent execution              |
| [#43](https://github.com/Interchouette-ITC/rustashop/issues/43) | AI-native commerce (API + UIs + MCP)                     |
| [#45](https://github.com/Interchouette-ITC/rustashop/issues/45) | Domains and `:dev` tip (`interchouette.net` / `.ai` / …) |

Child tasks use labels `area:wasm`, `area:realtime`, `area:extensions`, and `area:ai`. Filter the issues list by those labels for the full backlog.
