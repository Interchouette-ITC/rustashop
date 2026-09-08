# Architecture

rustashop is a Rust commerce product. The [Serenade](https://github.com/Interchouette-ITC/Serenade) framework supplies kernel concepts (DI, events, config, contracts). This repo owns commerce domain, persistence adapters, HTTP surfaces, shared templates, and shop hosts.

## Layers

```text
Clients (Angular | Leptos+rangular)
        │  OpenAPI + cart WebSocket
        ▼
rustashop-api (Actix)     rustashop-mcp (tool schema; Axum later)
        │
        ▼
rustashop-domain          pure types (Money, Product, Cart, Order, …)
        │
        ▼
rustashop-persist         feature-selected facade
   ┌────┴────┐
sqlx        seaorm
        │
        ▼
PostgreSQL
```

Shop markup/SCSS: `templates/shop/default/`. Hosts: `shops/angular`, `shops/leptos-rangular`.
Admin markup/SCSS: `templates/admin/default/`. Host: `admin/angular`.
Kinds (`shop` | `admin`) must not be mixed; see [`../templates/README.md`](../templates/README.md).

Serenade boots in the `rustashop` crate (`FrameworkBundle` + `RustashopBundle`, `config/packages`). Commerce HTTP binds through Serenade `listen` / `AsyncHttpKernel` (Actix adapter).

## Crates (today)

| Crate | Role |
| --- | --- |
| `rustashop` | App kernel: Serenade boot + DI container (`config/packages`) |
| `rustashop-domain` | Money, Product, Variant, Category, Cart, Order (no ORM types) |
| `rustashop-persist` | Facade: `persist-sqlx` (default) or `persist-seaorm` |
| `rustashop-persist-sqlx` | SQLx migrations, catalog/cart/order repos, migrate binary |
| `rustashop-persist-seaorm` | SeaORM mirror schema and repos |
| `rustashop-persist-diesel` | Experimental Diesel spike (`find_by_id` only; not facade-wired) |
| `rustashop-extensions` | WIT Component Model host (`pricing-adjust` invoke) |
| `rustashop-sandbox` | Wasmer polyglot sandbox host (Python `quote` fixture) |
| `rustashop-api` | Commerce HTTP via Serenade listen (Actix adapter), OpenAPI |
| `rustashop-mcp` | Tool schema for MCP / agents (`TOOLS`); Axum listen not wired yet |
| `rustashop-template-shop-default` | Shared storefront HTML/SCSS package |

## HTTP house split

| Surface | Framework | Owns |
| --- | --- | --- |
| Commerce API | **Serenade HttpKernel** (Actix listen adapter) + cart WebSocket | Catalog, cart, checkout, orders, admin REST; `GET /v1/carts/{id}/ws` push |
| MCP / tools | **Axum** | Streamable MCP and narrow agent endpoints |

Both share domain and persist. OpenAPI is generated with **utoipa** (`/openapi.json`). Regenerated file: `openapi/openapi.json` via `make openapi`.

## Request path (commerce)

```text
GET  /v1/products
POST /v1/carts → lines
POST /v1/checkout
  → serenade_http_actix / bind_commerce_server → AsyncHttpKernel
  → (mutations) CartHub → GET /v1/carts/{id}/ws
  → rustashop-api front controllers
  → serenade-contracts repository traits
  → Sqlx* | SeaOrm* adapters
  → PostgreSQL
```

Catalog → cart → checkout → order on the same stack. Messenger/events via Serenade when the kernel is wired.

## Persistence

- Postgres in Docker (`docker/compose.yml`); no host Postgres install.
- Dual backends behind one facade; enable exactly one of `persist-sqlx` / `persist-seaorm`.
- Diesel is an optional spike crate only ([ADR 0001](../docs-dev/adr/0001-diesel-persistence.md)); not selected by the facade.
- Repository traits come from **`serenade-contracts`**; adapters live here.

## Related surfaces

| Lane | Intent |
| --- | --- |
| Realtime | WebSocket gateway aligned with OpenAPI mutations |
| Extensions | WIT / Component Model hooks on **wasmtime** ([ADR 0002](../docs-dev/adr/0002-wit-plugin-engine.md)) |
| Sandbox vs native Python | Wasmer for untrusted scripts; PyO3 only for first-party connectors ([ADR 0003](../docs-dev/adr/0003-pyo3-vs-wasmer-sandbox.md)) |
| Sandbox | Wasmer (or similar) for untrusted / polyglot scripts |

Wasm roles (UI wasm vs plugins vs sandbox): [`docs-dev/WASM-LAYERS.md`](../docs-dev/WASM-LAYERS.md). Foundations: [`docs-dev/FOUNDATIONS.md`](../docs-dev/FOUNDATIONS.md).

## Local run

| Mode | Command |
| --- | --- |
| Full stack | `make stack-up` (Postgres + migrate + API on `8080`) |
| Host API | `make db-up && make db-migrate && make run-api` |
| Angular shop | `make shop-angular` (port `4242`) |
| Angular admin | `make admin-angular` (port `4250`) |
| Leptos shop | `make shop-leptos-rangular` (port `4181`) |

Do not bind `8080` twice. Details: [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Related

- Framework: [Serenade](https://github.com/Interchouette-ITC/Serenade)
- App kernel wire: issue [#49](https://github.com/Interchouette-ITC/rustashop/issues/49)
- Contributor docs epic: [#10](https://github.com/Interchouette-ITC/rustashop/issues/10)
