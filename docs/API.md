# Commerce HTTP API

The Actix commerce API is the single contract for storefronts, the Angular admin sample, and MCP proxies. Clients share OpenAPI; they do not invent parallel route shapes.

## Base URL and versioning

| Item | Value |
| --- | --- |
| Default bind | `127.0.0.1:8080` (`RUSTASHOP_BIND`) |
| Public JSON prefix | `/v1/...` |
| Liveness | `GET /healthz` |
| OpenAPI document | `GET /openapi.json` |

New breaking HTTP shapes get a new major prefix (`/v2/...`). Additive fields and new routes under `/v1` stay compatible for clients generated from the dump.

## Auth

| Surface | Auth |
| --- | --- |
| Storefront catalog, cart, checkout | Unauthenticated (cart uses a session `token` in the JSON body) |
| Admin routes under `/v1/{admin_api_prefix}/...` | `Authorization: Bearer <token>` |

The operator path segment defaults to `admin` (`RUSTASHOP_ADMIN_API_PREFIX`). The bearer value comes from install / env (`RUSTASHOP_ADMIN_API_TOKEN`). OpenAPI marks admin operations with the `admin_bearer` security scheme.

## OpenAPI dump and explorers

| Artefact | How |
| --- | --- |
| Checked-in dump | `openapi/openapi.json` |
| Regenerate | `make openapi` (fails CI on drift via `make openapi-check`) |
| Shop TypeScript types | `cd shops/angular && npm run generate:api` |

With feature `openapi-ui` (default on), listen mounts explorers next to the JSON:

| UI | URL |
| --- | --- |
| Swagger UI | `http://127.0.0.1:8080/swagger-ui/` |
| Redoc | `http://127.0.0.1:8080/redoc` |
| RapiDoc | `http://127.0.0.1:8080/rapidoc` |
| Scalar | `http://127.0.0.1:8080/scalar` |

Explorer mounts are provided by Serenade `serenade-openapi`. Path and schema annotations stay in `rustashop-api` (`ApiDoc`).

## Example curl

```bash
make db-up && make db-migrate && make db-seed
make run-api

curl -s http://127.0.0.1:8080/healthz
curl -s http://127.0.0.1:8080/v1/products | head

curl -s -X POST http://127.0.0.1:8080/v1/carts \
  -H 'content-type: application/json' \
  -d '{"currency":"EUR"}'

# Admin (replace token and prefix as configured)
curl -s http://127.0.0.1:8080/v1/admin/orders \
  -H "Authorization: Bearer $RUSTASHOP_ADMIN_API_TOKEN"
```

Idempotent checkout accepts header `Idempotency-Key`.

## WebSocket (cart session)

Live cart push sits beside the HTTP API on the same Actix process.

| Item | Value |
| --- | --- |
| Endpoint | `GET /v1/carts/{id}/ws?token=<cart.token>` |
| Auth | Opaque cart `token` from `CartResponse` (same as HTTP body token; not admin bearer) |
| Event | `cart.updated` JSON after cart mutations |

Example payload shape:

```json
{
  "type": "cart.updated",
  "version": 1,
  "cart": { "id": "…", "token": "…", "status": "open", "currency": "EUR", "lines": [], "items_total": { "amount_minor": 0, "currency": "EUR" } }
}
```

Clients must treat the server snapshot as authoritative. Integration coverage lives in `crates/rustashop-api/tests/cart_ws.rs`.

## WebSocket (admin order status)

| Item | Value |
| --- | --- |
| Endpoint | `GET /v1/{admin_api_prefix}/orders/{id}/ws?token=<admin bearer>` |
| Auth | Same admin bearer secret as HTTP operator routes (query `token`, mirror sandbox job WS) |
| Event | `order.updated` JSON after admin `PATCH …/orders/{id}` status change |

Example payload shape:

```json
{
  "type": "order.updated",
  "version": 1,
  "order": {
    "id": "…",
    "number": "…",
    "state": "shipped",
    "payment_status": "pending",
    "currency": "EUR",
    "items_total": { "amount_minor": 4500, "currency": "EUR" },
    "total": { "amount_minor": 4500, "currency": "EUR" },
    "lines": []
  }
}
```

Integration coverage: `crates/rustashop-api/tests/order_ws.rs`. Shop hosts do not subscribe yet (API-first).

Admin sandbox job logs use `GET /v1/{admin_api_prefix}/sandbox/jobs/{id}/ws`.

Design notes: [`docs-dev/REALTIME.md`](../docs-dev/REALTIME.md).

## Related

- Crate overview: [`ARCHITECTURE.md`](ARCHITECTURE.md)
- UI hosts on this API: [`UI.md`](UI.md)
- Screen × track parity: [`docs-dev/UI-RENDERERS.md`](../docs-dev/UI-RENDERERS.md)
- Local Make targets: [`CONTRIBUTING.md`](CONTRIBUTING.md)
