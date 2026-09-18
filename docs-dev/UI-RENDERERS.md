# UI clients and renderers

rustashop exposes **one Commerce API** (Actix + OpenAPI + WebSocket). All UIs are clients. This doc names the **UI plane** rustashop wants: Angular parity **and** rangular with **dual renderers** (web + native GPU).

## Ambition (rangular direction)

Similar in spirit to GPUix (“React’s model on GPUI”), rustashop targets:

> **Angular-shaped authoring across Rust web (Leptos/DOM) and native GPU (GPUI).**

That is **more ambitious than a webview desktop shell**: GPUI is a real native renderer, not Chrome embedded in Tauri.

| Track | Authoring                                   | Web renderer            | Native renderer                      |
| ----- | ------------------------------------------- | ----------------------- | ------------------------------------ |
| **A** | Angular (TypeScript)                        | Browser DOM             | (not in v0 scope; Ionic/`app` later) |
| **B** | **rangular** (templates + Rust controllers) | **Leptos** → DOM / wasm | **GPUI** → native GPU                |

Tracks A and B are developed **alike** against the same OpenAPI and realtime contracts: same cart, catalog, checkout flows; different toolchain.

## Layers (do not confuse)

```text
┌─────────────────────────────────────────────────────────────┐
│  UI clients                                                  │
│                                                              │
│  Angular (TS)          rangular (one authoring model)        │
│       │                      │                               │
│       │               ┌──────┴──────┐                        │
│       │               │             │                        │
│       │          Web renderer   Native renderer              │
│       │          Leptos / DOM      GPUI                      │
│       │               │             │                        │
│       └───────────────┴─────────────┘                        │
│                       │ OpenAPI + WS push                    │
└───────────────────────┼──────────────────────────────────────┘
                        ▼
┌─────────────────────────────────────────────────────────────┐
│  Commerce kernel (Serenade app + Actix API + Axum MCP)       │
└─────────────────────────────────────────────────────────────┘
```

**“Web backend Leptos”** and **“desktop backend GPUI”** mean the **UI host / renderer stack** for rangular components, **not** a replacement for the commerce API. Catalog, cart, checkout, and money stay on the Actix kernel.

## Web host: Leptos + rangular

| Mode                               | Role                                                         |
| ---------------------------------- | ------------------------------------------------------------ |
| **CSR wasm** (rangular v0.1 today) | Storefront in browser; Trunk/wasm; demo path exists upstream |
| **SSR / islands** (later)          | Optional Leptos server for SEO, first paint, admin shells    |

rustashop does not require Leptos full-stack monolith for the kernel. Leptos serves the **web UI host** when track B ships web surfaces.

## Native host: GPUI + rangular

| Item          | Note                                                                        |
| ------------- | --------------------------------------------------------------------------- |
| **Target**    | Desktop admin, operator tools, possibly storefront kiosk                    |
| **Renderer**  | [GPUI](https://github.com/zed-industries/gpui) (GPU-native UI)              |
| **Authoring** | Same rangular templates/controllers where the GPUI backend can compile them |
| **Not**       | Tauri/webview-only desktop (that remains “web path in a window”)            |

GPUI renderer work belongs primarily in **rangular** (new backend target). rustashop consumes it for admin and native commerce UX.

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
“rangular alone”: Leptos is the renderer we want to grow for shop **and**, when forms and native hosts are ready,
back-office.

## Admin (pluggable)

The back-office is **API-first**. Any SPA that speaks admin OpenAPI + auth may plug in (Angular, React, Vue, Leptos+rangular, …). rustashop ships an **Angular sample** as the default operator SPA; that is not a stack lock. A Leptos+rangular admin sample is **product work** (Host validators already landed upstream); native GPUI admin waits on [rangular #37](https://github.com/Interchouette-ITC/rangular/issues/37).

## Make targets (shops)

| Target                      | Role                                                    |
| --------------------------- | ------------------------------------------------------- |
| `make shop-angular`         | Serve Angular shop (port `4242` by default)             |
| `make shop-leptos-rangular` | Serve Leptos+rangular shop (Trunk; default port `4181`) |

Product vocabulary: **shop** (not storefront / vitrine).

**Note:** `shops/leptos-rangular` is excluded from the root Cargo workspace. Root `make test` does not run its tests; use `cd shops/leptos-rangular && cargo test` (and wasm build) for that host.

## Domains map (reminder)

| Host                    | Typical UI                                     |
| ----------------------- | ---------------------------------------------- |
| `rustashop.io` / `.dev` | Angular or Leptos+rangular **shop**            |
| Desktop installer       | rangular **native** (GPUI)                     |
| `rustashop.app`         | Ionic / mobile (later; likely Angular-aligned) |

## UI parity matrix (screens × track)

Honest status on org `dev` (not aspirational). Cell values: **shipped** | **partial** | **blocked** | **n/a**.

HTTP contract: [`docs/API.md`](../docs/API.md) and `openapi/openapi.json`. Cart push: `GET /v1/carts/{id}/ws` + `cart.updated`.

| Screen | OpenAPI (HTTP) | WS events | Angular shop | Leptos+rangular web | GPUI native | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Browse (product list) | `GET /v1/products` | — | shipped | shipped | blocked | Shared templates under `templates/shop/default/` |
| Product detail | `GET /v1/products/{id}` | — | shipped | shipped | blocked | Add-to-cart is HTTP today |
| Cart | `GET/POST /v1/carts…`, line PATCH/DELETE | `cart.updated` | shipped | partial | blocked | Angular WS [#263](https://github.com/Interchouette-ITC/rustashop/pull/263); Leptos WS [#265](https://github.com/Interchouette-ITC/rustashop/pull/265) when merged |
| Checkout | `POST /v1/checkout` | not yet (order lifecycle later) | shipped | missing | blocked | Leptos checkout is **product dogfood** (upstream Host validators shipped; [rangular #22](https://github.com/Interchouette-ITC/rangular/issues/22) **CLOSED**) |
| Admin orders list / status | `GET/PATCH /v1/{admin_api_prefix}/orders…` | not yet | shipped (`admin/angular`) | missing | blocked | Leptos admin = product sample; native waits [#37](https://github.com/Interchouette-ITC/rangular/issues/37) |

**missing** = not built in this host yet (no upstream forms blocker). **blocked** = waiting on GPUI (#37) or similar.

Admin sandbox job logs already use WS (`GET /v1/{admin_api_prefix}/sandbox/jobs/{id}/ws`) in `admin/angular` - that is not a storefront parity row.

## Upstream status

| Upstream | State | What it means for rustashop |
| --- | --- | --- |
| [rangular #22](https://github.com/Interchouette-ITC/rangular/issues/22) (forms / validators) | **CLOSED** (completed via [rangular #44](https://github.com/Interchouette-ITC/rangular/pull/44)) | Host `required` / `min_length` / `pattern` / `first_error` available; Leptos checkout/admin are product PRs, not “wait for #22” |
| [rangular #37](https://github.com/Interchouette-ITC/rangular/issues/37) (GPUI backend) | **OPEN** | Blocks **native** GPUI host only |

## Delivery order (current)

1. Commerce API + Angular shop + Leptos browse/cart - **landed**
2. UI parity docs + cart WS clients - N0–N2 ([#51](https://github.com/Interchouette-ITC/rustashop/issues/51), [#31](https://github.com/Interchouette-ITC/rustashop/issues/31))
3. Leptos checkout (+ optional Leptos admin sample) - product dogfood of closed #22 surface
4. Realtime deepen (inventory / order) - when ordered
5. rangular #37 - native admin/desktop

## Non-goals (early)

- One binary that is both Actix API and Leptos SSR for everything
- GPUI shop before admin proves the native renderer
- Mandating Angular (or any single SPA framework) for admin
- Forking Angular inside the Rust crates

## Related

- [WASM-LAYERS.md](WASM-LAYERS.md) - wasm roles (UI wasm vs plugins vs sandbox)
- [rangular SPEC](https://github.com/Interchouette-ITC/rangular/blob/dev/docs/SPEC.md) - v0.1 browser-only; GPUI is post-v0.1
- Contributor UI overview: [`docs/UI.md`](../docs/UI.md)
- Open issues: dual renderers [#50](https://github.com/Interchouette-ITC/rustashop/issues/50), realtime [#31](https://github.com/Interchouette-ITC/rustashop/issues/31)
