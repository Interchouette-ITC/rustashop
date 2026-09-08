# AI-native commerce

AI is **not** a bolt-on chatbot for rustashop. It is a first-class product axis across the **Commerce API** and both UI clients (Angular and rangular). Agents, MCP, and autonomous commerce workflows share the same domain, auth, realtime bus, and sandbox rules as the rest of the kernel.

## Product map

```text
rustashop
│
├── Commerce API
├── AI product discovery
├── AI shopping agents
├── AI-assisted catalog management
├── AI pricing / promotions
├── AI customer support
├── MCP integration
└── autonomous commerce agents
```

| Capability                     | Native meaning                                                                                   |
| ------------------------------ | ------------------------------------------------------------------------------------------------ |
| **Commerce API**               | Source of truth for catalog, cart, checkout, orders, money, inventory                            |
| **AI product discovery**       | Search / recommend / “shop for me” over the live catalog contract                                |
| **AI shopping agents**         | Session agents that browse, compare, fill cart under customer intent                             |
| **AI-assisted catalog**        | Merchant/admin agents that draft products, media, attributes (human commit)                      |
| **AI pricing / promotions**    | Propose rules and experiments; host applies after policy checks                                  |
| **AI customer support**        | Order-aware support with tools into the same API + realtime status                               |
| **MCP integration**            | Expose rustashop tools to external agents (and consume upstream MCP where useful)                |
| **Autonomous commerce agents** | Longer-running jobs (restock alerts, repricing drafts, migration assists) behind audit + sandbox |

## Backend (native)

- **Actix-web** hosts the commerce kernel: REST, OpenAPI, WebSocket gateway ([FOUNDATIONS.md](FOUNDATIONS.md)).
- Domain services expose **stable tool surfaces** (not ad-hoc prompts over SQL). See **[AI-TOOLS.md](AI-TOOLS.md)** for the v0 catalog (`rustashop-mcp::TOOLS`).
- Agent runs are **audited**; untrusted code/scripts go through the [Wasmer sandbox](WASMER-SANDBOX.md) lane (default unless a tool is marked first-party; [ADR 0003](adr/0003-pyo3-vs-wasmer-sandbox.md)).
- Money, inventory, and payment capture stay **host-authorized**; models propose, humans or strict policies confirm where required.
- Pricing / inventory **commits** stay host-mediated (same rule as WIT plugins).
- Realtime gateway carries agent job progress and cart updates ([REALTIME.md](REALTIME.md)).
- **Axum** hosts MCP Streamable HTTP (`make run-mcp-http`, default `127.0.0.1:8090/mcp`) or stdio (`make run-mcp`); handlers proxy to the Actix API.

## Frontend (native)

- Angular admin: agent console, catalog assist, support workspace (not a separate “AI product”).
- Storefront (Angular or rangular): discovery and shopping-agent UX on the same OpenAPI + push channel.
- Ionic app (`rustashop.app` later): same contracts; mobile-first agent surfaces.

## Trust boundary

| Trusted                                            | Untrusted / mediated                                 |
| -------------------------------------------------- | ---------------------------------------------------- |
| First-party model calls inside the API with policy | Merchant-uploaded scripts (Wasmer)                   |
| Official MCP tools with authz                      | Raw model output applied to money without review     |
| Human approve for catalog publish / capture        | Autonomous capture or inventory write without guards |

## Delivery slices

1. **Done (#44):** tool schema for cart/catalog/order reads + draft writes - [AI-TOOLS.md](AI-TOOLS.md) + `rustashop-mcp`
2. Admin agent console (Angular) beyond `/sandbox` + job audit
3. Storefront discovery / shopping-agent MVP
4. **Done (#47):** MCP HTTP surface on **Axum**, tools from the same catalog
5. Autonomous job runner (sandbox + host commit)
6. Model provider config (operator API keys; never in Wasmer guests)

Track under the AI epic on GitHub (`area:ai`).
