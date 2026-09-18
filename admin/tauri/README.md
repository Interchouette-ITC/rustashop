# rustashop-admin-tauri

Tauri 2 operator shell: native menu + webview loading `admin/leptos-rangular`.

## Run

```bash
# API on :8080 (RUSTASHOP_ADMIN_API_TOKEN set)
make run-api
make admin-tauri          # Trunk serve + debug Tauri window
make build-admin-tauri    # Trunk release + release binary (no open)
```

Browser path stays `make admin-leptos-rangular` (port 4251). Desktop reuses the same wasm.

### API base

- **Dev (Trunk):** same-origin `/api` proxy to Actix (`:8080`).
- **Desktop / non-http origin:** defaults to `http://127.0.0.1:8080`. Override with `sessionStorage` key `rs.adminApiBase` (no trailing slash), e.g. a tip URL.

## Test / lint

```bash
make test-admin-tauri
make lint-admin-tauri
```

CI runs `cargo check` + clippy on this crate (no GUI). Full window smoke is local (`make admin-tauri`).
