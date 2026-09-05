# Architecture

rustashop is a Rust commerce product. The [Serenade](https://github.com/Interchouette-ITC/Serenade) framework supplies kernel concepts (DI, events, config, contracts). This repo owns commerce domain, persistence adapters, HTTP surfaces, shared templates, and shop hosts.

## Layers

```text
Clients (Angular | Leptos+rangular)
        │  OpenAPI (+ WebSocket later)
        ▼
rustashop-api (Actix)     rustashop-mcp (Axum name marker)
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
| `rustashop-api` | Commerce HTTP via Serenade listen (Actix adapter), OpenAPI |
| `rustashop-mcp` | Axum MCP / agent tools (name marker; not wired yet) |
| `rustashop-template-shop-default` | Shared storefront HTML/SCSS package |

## HTTP house split

| Surface | Framework | Owns |
| --- | --- | --- |
| Commerce API | **Serenade HttpKernel** (Actix listen adapter) | Catalog, cart, checkout, orders, admin REST; WebSocket later |
| MCP / tools | **Axum** | Streamable MCP and narrow agent endpoints |

Both share domain and persist. OpenAPI is generated with **utoipa** (`/openapi.json`). Regenerated file: `openapi/openapi.json` via `make openapi`.

## Request path (commerce)

```text
GET  /v1/products
POST /v1/carts → lines
POST /v1/checkout
  → serenade_http_actix::listen → AsyncHttpKernel
  → rustashop-api front controllers
  → serenade-contracts repository traits
  → Sqlx* | SeaOrm* adapters
  → PostgreSQL
```

Catalog → cart → checkout → order on the same stack. Messenger/events via Serenade when the kernel is wired.

## Persistence

- Postgres in Docker (`docker/compose.yml`); no host Postgres install.
- Dual backends behind one facade; enable exactly one of `persist-sqlx` / `persist-seaorm`.
- Diesel is deferred (separate issue).
- Repository traits come from **`serenade-contracts`**; adapters live here.

## Related surfaces

| Lane | Intent |
| --- | --- |
| Realtime | WebSocket gateway aligned with OpenAPI mutations |
| Extensions | WIT / Component Model host hooks |
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
