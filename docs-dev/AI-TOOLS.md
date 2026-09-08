# Commerce AI tool schema (v0)

Stable **tool / function** names for MCP and in-app agents. They map 1:1 to OpenAPI commerce routes. Models never receive raw SQL or invent money floats.

Typed catalog: `rustashop-mcp` (`TOOLS`, `tools_catalog_json()`, input structs).

Parent epic: [#43](https://github.com/Interchouette-ITC/rustashop/issues/43). Slice: [#44](https://github.com/Interchouette-ITC/rustashop/issues/44).

## Rules

| Rule | Meaning |
| --- | --- |
| Integer money | Amounts are `amount_minor` + ISO `currency` (same as OpenAPI) |
| No SQL-from-prompt | Tools call host handlers / repositories only |
| Draft vs commit | Cart line edits are **draft**; checkout and order status are **commit** |
| Autonomous gate | Commit tools set `human_approve_for_autonomous` (policy or human before run) |
| Sandbox | Untrusted merchant scripts stay on Wasmer (`rustashop-sandbox` / Serenade sandbox host); first-party tools are trusted host path ([ADR 0003](adr/0003-pyo3-vs-wasmer-sandbox.md)) |

## Scopes

| Scope | Auth |
| --- | --- |
| `shop` | Public catalog; cart/checkout with cart session semantics |
| `admin` | `Authorization: Bearer` + opaque admin URI prefix |

## Effects

| Effect | Examples |
| --- | --- |
| `read` | `list_products`, `get_cart`, `list_admin_orders` |
| `draft_write` | `create_cart`, `add_cart_line`, `update_cart_line`, `delete_cart_line` |
| `commit` | `place_order`, `patch_order_status` |

## Tool table (v0)

| Name | Method | OpenAPI path | Scope | Effect | Autonomous approve |
| --- | --- | --- | --- | --- | --- |
| `list_products` | GET | `/v1/products` | shop | read | no |
| `get_product` | GET | `/v1/products/{id}` | shop | read | no |
| `create_cart` | POST | `/v1/carts` | shop | draft_write | no |
| `get_cart` | GET | `/v1/carts/{id}` | shop | read | no |
| `add_cart_line` | POST | `/v1/carts/{id}/lines` | shop | draft_write | no |
| `update_cart_line` | PATCH | `/v1/carts/{id}/lines/{line_id}` | shop | draft_write | no |
| `delete_cart_line` | DELETE | `/v1/carts/{id}/lines/{line_id}` | shop | draft_write | no |
| `place_order` | POST | `/v1/checkout` | shop | commit | **yes** |
| `list_admin_products` | GET | `/v1/{admin_api_prefix}/products` | admin | read | no |
| `list_admin_orders` | GET | `/v1/{admin_api_prefix}/orders` | admin | read | no |
| `patch_order_status` | PATCH | `/v1/{admin_api_prefix}/orders/{id}` | admin | commit | **yes** |

## Related host surfaces (not MCP tools yet)

| Surface | Role |
| --- | --- |
| Sandbox jobs HTTP + WS | Untrusted script runs (`/v1/{admin}/sandbox/jobs`, job WS) |
| Cart WS | `GET /v1/carts/{id}/ws` live cart push |
| WIT `pricing-adjust` | Plugin lane (Component Model), not an MCP tool |

## Next slices

1. Axum MCP server in `rustashop-mcp` exposing this catalog (#47).
2. Admin / shop AI UIs calling the same names.
3. Provider API-key routing for first-party model calls (keys never in Wasmer guests).
4. Autonomous jobs: sandbox + host commit using Serenade messenger when wired.

## Non-goals (v0)

- Catalog publish / invent product write tools
- Inventory adjustment tools
- Payment capture tools
- Raw SQL or ad-hoc query tools
