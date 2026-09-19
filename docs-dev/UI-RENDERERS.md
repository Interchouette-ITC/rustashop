# UI clients and renderers

rustashop exposes **one Commerce API** (Actix + OpenAPI + WebSocket). All UIs are clients.

The UI plane is:

- **Shop / admin web:** Angular (track A) and Leptos + rangular (track B), including Tauri webview shells around the same wasm
- **Native GPUI apps:** separate hand-written Rust binaries for **ops** (logistics / back-office) and **POS / TPV** (caisse). Not a rangular dual-renderer.

## Ambition

| Surface                | Authoring                             | Host                                                   | Role                                                       |
| ---------------------- | ------------------------------------- | ------------------------------------------------------ | ---------------------------------------------------------- |
| **Shop A**             | Angular (TypeScript)                  | Browser DOM                                            | Customer storefront                                        |
| **Shop B**             | rangular templates + Rust controllers | **Leptos** → DOM / wasm (+ optional **Tauri** webview) | Same OpenAPI shop flows                                    |
| **Admin samples**      | Angular or Leptos+rangular            | Browser / Tauri webview                                | Operator SPA samples                                       |
| **Ops (native)**       | Hand-written Rust UI                  | **GPUI**                                               | Orders, stock, catalog sync (back-of-house)                |
| **POS / TPV (native)** | Hand-written Rust UI                  | **GPUI**                                               | Encaissement, ticket, fiscal journal path (front-of-house) |

Tracks A and B are developed **alike** against the same OpenAPI and realtime contracts for **shop** screens. GPUI apps are **different jobs** on the same API, not “shop templates on GPU”.

## Layers (do not confuse)

```text
┌─────────────────────────────────────────────────────────────┐
│  UI clients                                                  │
│                                                              │
│  Angular (TS)     Leptos+rangular      GPUI ops    GPUI POS  │
│  shop / admin     shop / admin         (native)    (native)  │
│       │                │                   │           │     │
│       │           (+ Tauri webview)        │           │     │
│       └────────────────┴───────────────────┴───────────┘     │
│                          │ OpenAPI + WS push                 │
└──────────────────────────┼───────────────────────────────────┘
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  Commerce kernel (Serenade app + Actix API + Axum MCP)       │
└─────────────────────────────────────────────────────────────┘
```

GPUI does **not** compile rangular templates. Tauri is a **webview** around Leptos wasm (shop/admin). Catalog, cart, checkout, orders, and money stay on the Actix kernel.

## Web host: Leptos + rangular

| Mode                               | Role                                                         |
| ---------------------------------- | ------------------------------------------------------------ |
| **CSR wasm** (rangular v0.1 today) | Storefront in browser; Trunk/wasm; demo path exists upstream |
| **SSR / islands** (later)          | Optional Leptos server for SEO, first paint, admin shells    |

rustashop does not require Leptos full-stack monolith for the kernel. Leptos serves the **web UI host** when track B ships web surfaces. Desktop shop/admin installers reuse that wasm via **Tauri**.

## Native host: GPUI (ops + POS)

| Item                   | Note                                                                                                      |
| ---------------------- | --------------------------------------------------------------------------------------------------------- |
| **Ops / logistique**   | Gestion commerciale / WMS light: orders list + status PATCH, stock / mouvements, catalog sync             |
| **Caisse / TPV / POS** | Encaissement, ticket, online catalog sync; FR fiscal path (NF525 / LNE: inaltérabilité, journal, clôture) |
| **Renderer**           | [GPUI](https://github.com/zed-industries/gpui) (GPU-native UI)                                            |
| **Authoring**          | Hand-written Rust GPUI views (not rangular templates)                                                     |
| **Not**                | Tauri webview shop clone; not “Angular admin rewritten in GPUI” as the only story                         |

Both binaries speak the same Actix / OpenAPI. They are not shipped yet (`missing` in the matrix below).

## Angular track (parallel)

Angular remains **UI option A**: mature SPA under `shops/angular`. Controllers
differ by stack; **template markup is shared** under `templates/<id>/`
(rangular subset). Also share:

- Same API types (OpenAPI codegen)
- Same WS event names
- Same browse → cart → checkout flows

Default template package: `templates/shop/default` (`@rustashop/template-shop-default`).
Hosts keep controllers only; build adapters land in each shop’s `generated/`
(gitignored).

## Leptos + rangular track

Track B is **Leptos as the web host** (CSR wasm today) with **rangular** Host
controllers and the same `templates/<id>/` files as Angular. Make target:
`make shop-leptos-rangular`. Path: `shops/leptos-rangular`. It is not
“rangular alone”: Leptos is the browser renderer for shop **and** the Leptos admin sample.

## Admin (pluggable)

The back-office is **API-first**. Any SPA that speaks admin OpenAPI + auth may plug in (Angular, React, Vue, Leptos+rangular, …). rustashop ships an **Angular sample** (`admin/angular`) and a **Leptos+rangular sample** (`admin/leptos-rangular`, orders + products). Desktop operators can run the same Leptos wasm in **Tauri** (`admin/tauri`, `make admin-tauri`). Native **GPUI ops** is a separate client (planned), not a wait on a rangular GPUI backend.

## Make targets (shops)

| Target                       | Role                                                     |
| ---------------------------- | -------------------------------------------------------- |
| `make shop-angular`          | Serve Angular shop (port `4242` by default)              |
| `make shop-leptos-rangular`  | Serve Leptos+rangular shop (Trunk; default port `4181`)  |
| `make admin-angular`         | Serve Angular admin (port `4250` by default)             |
| `make admin-leptos-rangular` | Serve Leptos+rangular admin (Trunk; default port `4251`) |
| `make admin-tauri`           | Tauri 2 desktop shell around the Leptos admin wasm       |
| `make shop-tauri`            | Tauri 2 desktop shop install around the Leptos shop wasm |

Product vocabulary: **shop** (not storefront / vitrine). Ops and POS are not shops.

**Note:** `shops/leptos-rangular` is its own Cargo workspace (not a root member). Root `make test` runs it via `make test-shops` (`make test-shop-leptos`). Clippy for that host is `make lint-shop-leptos` (also from `make lint`).

## Domains map (reminder)

| Host                    | Typical UI                                         |
| ----------------------- | -------------------------------------------------- |
| `rustashop.io` / `.dev` | Angular or Leptos+rangular **shop**                |
| Desktop installer       | Tauri webview (shop + admin); later GPUI ops / POS |
| `rustashop.app`         | Ionic / mobile (later; likely Angular-aligned)     |

## UI parity matrix (screens × track)

Honest status on org `dev` (not aspirational). Cell values: **shipped** | **partial** | **missing** | **n/a**.

HTTP contract: [`docs/API.md`](../docs/API.md) and `openapi/openapi.json`. Cart push: `GET /v1/carts/{id}/ws` + `cart.updated`. Order push (admin): `GET /v1/{admin}/orders/{id}/ws` + `order.updated` (API-first; shop clients later).

| Screen                     | OpenAPI (HTTP)                             | WS events               | Angular shop                   | Leptos+rangular web                                 | GPUI      | Notes                                                                                                                                                                                            |
| -------------------------- | ------------------------------------------ | ----------------------- | ------------------------------ | --------------------------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Browse (product list)      | `GET /v1/products`                         | —                       | shipped                        | shipped                                             | n/a       | Shop only; GPUI ops/POS sync catalog differently                                                                                                                                                 |
| Product detail             | `GET /v1/products/{id}`                    | —                       | shipped                        | shipped                                             | n/a       | Add-to-cart is HTTP today                                                                                                                                                                        |
| Cart                       | `GET/POST /v1/carts…`, line PATCH/DELETE   | `cart.updated`          | shipped                        | shipped                                             | n/a       | Angular WS [#263](https://github.com/Interchouette-ITC/rustashop/pull/263); Leptos WS [#265](https://github.com/Interchouette-ITC/rustashop/pull/265)                                            |
| Checkout                   | `POST /v1/checkout`                        | —                       | shipped                        | shipped                                             | n/a       | Leptos checkout dogfoods Host validators ([rangular #22](https://github.com/Interchouette-ITC/rangular/issues/22) **CLOSED**); [#268](https://github.com/Interchouette-ITC/rustashop/issues/268) |
| Admin orders list / status | `GET/PATCH /v1/{admin_api_prefix}/orders…` | `order.updated` (API)   | shipped (`admin/angular` HTTP) | partial (`admin/leptos-rangular` orders + products) | n/a (SPA) | Leptos admin web [#272](https://github.com/Interchouette-ITC/rustashop/issues/272) **shipped**; Tauri desktop [#274](https://github.com/Interchouette-ITC/rustashop/issues/274)                  |
| Ops orders / stock         | Admin orders + inventory APIs              | `order.updated` (later) | n/a                            | n/a                                                 | missing   | Planned GPUI ops binary (back-of-house)                                                                                                                                                          |
| POS sale / ticket          | Commerce + fiscal journal APIs             | —                       | n/a                            | n/a                                                 | missing   | Planned GPUI POS / TPV (front-of-house; NF525 path)                                                                                                                                              |

**missing** = not built in this host yet. **partial** = subset of Angular admin screens on Leptos (orders + products; agents/sandbox still Angular). **n/a** = wrong surface for that host.

| Host                         | Make                         | Role                                                                                  |
| ---------------------------- | ---------------------------- | ------------------------------------------------------------------------------------- |
| Leptos admin (browser)       | `make admin-leptos-rangular` | Trunk, default port `4251`; bearer + `/api` proxy                                     |
| Admin Tauri (desktop)        | `make admin-tauri`           | Same admin wasm in webview; File/Edit/View/Help; API base via bar / `rs.adminApiBase` |
| Leptos shop (browser)        | `make shop-leptos-rangular`  | Trunk, default port `4181`; `/api` proxy                                              |
| Shop Tauri (desktop install) | `make shop-tauri`            | Same shop wasm for customer PC shopping; API base via bar / `rs.shopApiBase`          |
| Angular admin                | `make admin-angular`         | Full sample (orders, products, agents, sandbox)                                       |
| GPUI ops                     | (planned)                    | Native logistics / commercial back-office                                             |
| GPUI POS                     | (planned)                    | Native caisse / TPV                                                                   |

Desktop Tauri is a **webview** around Leptos wasm (not GPUI). Merchants may ship the shop Tauri binary so customers install a heavy desktop shop pointed at that merchant’s Commerce API; browser Trunk remains the default web path.

Admin sandbox job logs already use WS (`GET /v1/{admin_api_prefix}/sandbox/jobs/{id}/ws`) in `admin/angular` - that is not a storefront parity row.

## Upstream status

| Upstream                                                                                     | State                                                                                            | What it means for rustashop                                                                                                      |
| -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------- |
| [rangular #22](https://github.com/Interchouette-ITC/rangular/issues/22) (forms / validators) | **CLOSED** (completed via [rangular #44](https://github.com/Interchouette-ITC/rangular/pull/44)) | Host `required` / `min_length` / `pattern` / `first_error` available; Leptos checkout/admin are product work, not “wait for #22” |
| [rangular #37](https://github.com/Interchouette-ITC/rangular/issues/37) (GPUI backend)       | **OPEN** (upstream)                                                                              | **Not** a rustashop delivery gate. rustashop GPUI apps do not consume rangular                                                   |

## Delivery order (current)

1. Commerce API + Angular shop + Leptos browse/cart - **landed**
2. UI parity docs + cart WS clients - landed ([#51](https://github.com/Interchouette-ITC/rustashop/issues/51), [#31](https://github.com/Interchouette-ITC/rustashop/issues/31))
3. Leptos checkout - **shipped** ([#268](https://github.com/Interchouette-ITC/rustashop/issues/268))
4. Admin Leptos + Tauri shop/admin - **shipped**
5. Realtime deepen (inventory / clients) - when ordered
6. GPUI ops + GPUI POS / TPV - when ordered (separate binaries)

## Non-goals (early)

- One binary that is both Actix API and Leptos SSR for everything
- Compiling rangular templates to GPUI for shop or admin
- Treating Tauri webview shop as POS / NF525 caisse
- Mandating Angular (or any single SPA framework) for admin
- Forking Angular inside the Rust crates
- Claiming NF525 / LNE certification before a real fiscal journal and audit path exist

## Related

- [WASM-LAYERS.md](WASM-LAYERS.md) - wasm roles (UI wasm vs plugins vs sandbox)
- [rangular SPEC](https://github.com/Interchouette-ITC/rangular/blob/dev/docs/SPEC.md) - v0.1 browser-only
- Contributor UI overview: [`docs/UI.md`](../docs/UI.md)
- Open issues: UI hosts [#50](https://github.com/Interchouette-ITC/rustashop/issues/50), realtime [#31](https://github.com/Interchouette-ITC/rustashop/issues/31)
