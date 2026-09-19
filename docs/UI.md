# UI tracks

One commerce API. Two storefront hosts. Admin SPA samples (Angular + Leptos). Leptos is the **web renderer** for the rangular track. Native **GPUI** apps (ops + POS / TPV) are separate clients, not a second rangular renderer.

## Same contract

Every host talks to the Actix API described in [`API.md`](API.md):

| Concern                    | Source of truth            |
| -------------------------- | -------------------------- |
| HTTP shapes                | `openapi/openapi.json`     |
| Shared shop markup / SCSS  | `templates/shop/default/`  |
| Shared admin markup / SCSS | `templates/admin/default/` |

Do not hand-edit generated host output under `shops/*/generated/` or `admin/*/generated/` (build artefacts).

## Track A: Angular storefront

| Item         | Value                                          |
| ------------ | ---------------------------------------------- |
| Path         | `shops/angular`                                |
| Local URL    | `http://127.0.0.1:4242/` (`make shop-angular`) |
| Client types | `npm run generate:api` from the OpenAPI dump   |

Angular is the full storefront sample (catalog, cart, checkout) on shared templates.

## Track B: Leptos + rangular

| Item      | Value                                                  |
| --------- | ------------------------------------------------------ |
| Path      | `shops/leptos-rangular`                                |
| Local URL | `http://127.0.0.1:4181/` (`make shop-leptos-rangular`) |
| Role      | Same templates compiled toward a Rust/Wasm web host    |

**rangular** is the template / AOT tooling. **Leptos** is the browser (DOM) renderer for that track (also wrapped in Tauri webview for desktop shop/admin). GPUI ops and POS binaries are hand-written native apps on the same OpenAPI; they do not compile rangular templates.

## Admin sample

| Item      | Value                                                |
| --------- | ---------------------------------------------------- |
| Path      | `admin/angular`                                      |
| Local URL | `http://127.0.0.1:4250/` (`make admin-angular`)      |
| Auth      | Paste the admin bearer (`RUSTASHOP_ADMIN_API_TOKEN`) |

Orders list and status PATCH use the same `/v1/{admin_api_prefix}/...` routes as the OpenAPI admin tags.

## Choosing a path

| Goal                        | Start here                                              |
| --------------------------- | ------------------------------------------------------- |
| Familiar TypeScript SPA     | Track A (`shops/angular`)                               |
| Rust/Wasm shop host         | Track B (`shops/leptos-rangular`)                       |
| Operator back-office sample | `admin/angular` or `admin/leptos-rangular`              |
| Native logistics / stock    | GPUI ops (`make ops-gpui`; partial)                     |
| Native caisse / TPV / POS   | GPUI POS (`make pos-gpui`; partial, not certified)      |
| Change shared layout/CSS    | `templates/shop/default/` or `templates/admin/default/` |

API work stays in `crates/rustashop-api` and the OpenAPI dump. UI hosts consume; they do not redefine commerce routes.

## Parity matrix

Which screens exist on which host (honest `shipped` / `partial` / `missing` / `n/a`):

[`docs-dev/UI-RENDERERS.md`](../docs-dev/UI-RENDERERS.md) - section **UI parity matrix**.

## Related

- HTTP, OpenAPI, and cart WebSocket: [`API.md`](API.md)
- Crate map: [`ARCHITECTURE.md`](ARCHITECTURE.md)
- Renderer detail + blockers: [`../docs-dev/UI-RENDERERS.md`](../docs-dev/UI-RENDERERS.md)
