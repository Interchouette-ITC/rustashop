# rustashop-shop-tauri

Tauri 2 customer shop shell: native menu + webview loading `shops/leptos-rangular`.

Merchants can offer this installable desktop shop; the UI is the same Leptos+rangular storefront (browse → cart → checkout + cart WebSocket) pointed at that merchant’s Commerce API.

## Run

```bash
# API on :8080
make run-api
make shop-tauri            # Trunk serve + debug Tauri window
make build-shop-tauri      # Trunk release + release binary (no open)
```

Browser path stays `make shop-leptos-rangular` (port 4181).

### API base

- **Dev (Trunk):** same-origin `/api` proxy to Actix (`:8080`).
- **Desktop / non-http origin:** defaults to `http://127.0.0.1:8080`. Override with the shell bar or `sessionStorage` key `rs.shopApiBase` (no trailing slash), e.g. a merchant tip URL.

## Test / lint

```bash
make test-shop-tauri
make lint-shop-tauri
```

CI runs `cargo check` + clippy (no GUI). Full window smoke is local (`make shop-tauri`).
