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

The back-office is **API-first**. Any SPA that speaks admin OpenAPI + auth may plug in (Angular, React, Vue, Leptos+rangular, …). rustashop ships an **Angular sample** as the default operator SPA; that is not a stack lock. Leptos+rangular admin follows once forms (rangular #22) and optionally GPUI (#37) are ready.

## Make targets (shops)

| Target                      | Role                                                    |
| --------------------------- | ------------------------------------------------------- |
| `make shop-angular`         | Serve Angular shop (port `4242` by default)             |
| `make shop-leptos-rangular` | Serve Leptos+rangular shop (Trunk; default port `4181`) |

Product vocabulary: **shop** (not storefront / vitrine).

## Domains map (reminder)

| Host                    | Typical UI                                     |
| ----------------------- | ---------------------------------------------- |
| `rustashop.io` / `.dev` | Angular or Leptos+rangular **shop**            |
| Desktop installer       | rangular **native** (GPUI)                     |
| `rustashop.app`         | Ionic / mobile (later; likely Angular-aligned) |

## Dependencies and order

Upstream rangular work splits into **two blockers** with different blast radius:

| Upstream                                                                                     | Blocks                                                                                            | Does not block                                                                                |
| -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| [rangular #22](https://github.com/Interchouette-ITC/rangular/issues/22) (forms / validators) | Leptos+rangular **checkout**, **admin CRUD**, multi-field UX on **any** renderer (Leptos or GPUI) | API; Angular shop; Leptos+rangular **browse-only** (catalog list/detail, add-to-cart buttons) |
| [rangular #37](https://github.com/Interchouette-ITC/rangular/issues/37) (GPUI backend)       | rangular **native** host only                                                                     | Leptos web shop; Angular; API                                                                 |

Suggested order:

1. **Commerce API** (landed through cart/checkout)
2. **Angular shop** (#21 scaffold, #22 pages) - stable SPA path
3. **rangular #22** (forms) - unlocks Leptos+rangular checkout **and** future BO
4. **Leptos+rangular shop** browse + cart (#23, #24)
5. **Admin API** + pluggable SPA sample (#6); Angular sample first, Leptos+rangular BO when ready
6. **rangular #37** (GPUI) - native admin/desktop

## Non-goals (early)

- One binary that is both Actix API and Leptos SSR for everything
- GPUI shop before admin proves the native renderer
- Mandating Angular (or any single SPA framework) for admin
- Forking Angular inside the Rust crates

## Related

- [WASM-LAYERS.md](WASM-LAYERS.md) - wasm roles (UI wasm vs plugins vs sandbox)
- [rangular SPEC](https://github.com/Interchouette-ITC/rangular/blob/dev/docs/SPEC.md) - v0.1 browser-only; GPUI is post-v0.1
- GitHub: rustashop UI / shop epics (#7, #8, #6)
