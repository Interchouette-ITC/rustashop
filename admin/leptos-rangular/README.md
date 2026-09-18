# admin-leptos-rangular

Leptos CSR operator admin (track B) using styles from `templates/admin/default`.

## Run

```bash
# API on :8080 (RUSTASHOP_ADMIN_API_TOKEN set)
make run-api
make admin-leptos-rangular   # http://127.0.0.1:4251/
make admin-tauri             # same wasm in a Tauri window
```

Paste the same bearer as Angular admin. Screens: Orders (list + status PATCH), Products (list).

API base defaults to `/api` (Trunk proxy) in the browser, or `http://127.0.0.1:8080` on non-http origins. Override with the bar or `sessionStorage` key `rs.adminApiBase`.

## Test / lint

```bash
make test-admin-leptos
make lint-admin-leptos
```

Angular admin (`make admin-angular`) remains the full sample (agents, sandbox, …).
