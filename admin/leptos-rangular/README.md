# admin-leptos-rangular

Leptos CSR operator admin (track B) using styles from `templates/admin/default`.

## Run

```bash
# API on :8080 (RUSTASHOP_ADMIN_API_TOKEN set)
make run-api
make admin-leptos-rangular   # http://127.0.0.1:4251/
```

Paste the same bearer as Angular admin. Screens: Orders (list + status PATCH), Products (list).

## Test / lint

```bash
make test-admin-leptos
make lint-admin-leptos
```

Angular admin (`make admin-angular`) remains the full sample (agents, sandbox, …).
