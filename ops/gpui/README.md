# rustashop ops (GPUI)

Native **back-of-house** desktop client: orders (list + status PATCH) and catalog sync
(products + variant stock from public product detail). Speaks the same Actix admin
OpenAPI as `admin/angular` / `admin/leptos-rangular`.

Not a shop. Not Tauri. Not rangular templates.

## Run

```bash
# API must be up (e.g. make run-api) with admin token set
export RUSTASHOP_ADMIN_API_TOKEN=…
make ops-gpui
```

Overrides:

| Env / flag | Role |
| --- | --- |
| `RUSTASHOP_API_BASE` / `--api-base` | Actix base (default `http://127.0.0.1:8080`) |
| `RUSTASHOP_ADMIN_API_PREFIX` / `--admin-prefix` | Admin path segment (default `admin`) |
| `RUSTASHOP_ADMIN_API_TOKEN` / `--token` | Bearer (required) |

On weak GPUs, set `WGPU_BACKEND=gl` before launch if the native GPU backend fails.

## Gates

```bash
make test-ops-gpui   # unit tests (no window)
make lint-ops-gpui   # clippy
```

## Stock / mouvements

There is no dedicated stock-movements API yet. The Catalog tab shows **variant
`stock_quantity`** from `GET /v1/products/{id}` after an admin product list sync.
Do not treat this UI as a WMS ledger.
