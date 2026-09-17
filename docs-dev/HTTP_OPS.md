# HTTP ops (commerce API)

Probes and request correlation on `rustashop-api` listen.

| Path | Role | Body |
| --- | --- | --- |
| `GET /healthz` | Liveness (OpenAPI + JSON) | `{"status":"ok",…}` |
| `GET /readyz` | Load-balancer readiness | plain `ready` / `503 not ready` (Serenade `Readiness`) |

`/healthz` stays the product JSON contract. `/readyz` uses Serenade readiness helpers so operators can fail probes before drain without changing the OpenAPI liveness schema.

## Request id

`AsyncRequestIdMiddleware` on the Serenade `AsyncHttpKernel` propagates or generates `x-request-id` and stores it on the request attributes. Dispatch logs may include `request_id` on the `serenade::request` target.

## Admin security

Commerce kernel runs an async firewall (Serenade `Authenticator` + `_security_token` attribute) with anonymous allowed for public routes. Admin routes still require `Authorization: Bearer <token>` matching `RUSTASHOP_ADMIN_API_TOKEN` / `ADMIN_API_TOKEN`, checked via `AccessDecisionManager` / `RoleVoter` on subject `admin.area`. The Angular admin SPA keeps bearer auth.

## Session bridge

`AsyncSessionMiddleware` + `AsyncSessionTokenMiddleware` run on the commerce kernel (cookie `SERENADE_SESSION`). Browser dogfood routes:

| Path | Role |
| --- | --- |
| `GET /session` | Show firewall vs session authenticated flags |
| `GET`/`POST /session/login` | HTML form + CSRF; `login()` sticks admin identity |
| `GET`/`POST /session/logout` | HTML form + CSRF; `logout()` clears identity |
| `GET`/`POST /install/form` | HTML install confirm with `HmacCsrfTokenManager` |

CSRF secret: `RUSTASHOP_CSRF_SECRET`, else `RUSTASHOP_ADMIN_API_TOKEN`, else a local dev default.

**Cart `token` is not this cookie.** Cart lines and cart WebSocket auth stay domain-owned opaque secrets in the database. Serenade session is HTTP identity stickiness only.

## Catalog cache

Storefront `GET /v1/products` and `GET /v1/products/{id}` use `serenade-cache` (`ArrayAdapter` in-process by default). Entries are tagged `catalog`. Admin `PATCH /v1/{admin}/products/{id}` (enabled flag) invalidates that tag.

| Env | Role |
| --- | --- |
| `RUSTASHOP_CATALOG_CACHE_TTL_SECS` | TTL seconds (default `30`; `0` = no TTL besides tag invalidate) |

Multi-node: swap the pool for Serenade `RedisAdapter` (crate feature `redis`) with the same tag contract.

## Public write rate limits

Mutating cart and checkout routes (`POST /v1/carts`, cart line mutations, `POST /v1/checkout`) consume a Serenade fixed-window limiter (in-memory by default). Client key: `X-Forwarded-For` (first hop), else `X-Real-IP`, else `X-Client-Id`, else `anon`. Rejected requests return `429` with `Retry-After` / `X-RateLimit-*`.

| Env | Role |
| --- | --- |
| `RUSTASHOP_PUBLIC_RATE_LIMIT` | Tokens per window (default `120`; `0` disables) |
| `RUSTASHOP_PUBLIC_RATE_WINDOW_SECS` | Window length (default `60`) |

Multi-node: use Serenade `RedisRateLimiterStorage` (crate feature `redis`) instead of `InMemoryRateLimiterStorage`.

## Logging

Process logging goes through `serenade-observability` (`LoggingConfig` + `init`). Default sinks: stderr and rolling files under `var/log/` (stem from `RUSTASHOP_ENV`, default `dev`). Filter: `SERENADE_LOG` or `RUST_LOG`.

Optional feature `otel` on `rustashop-api` enables the OpenTelemetry bridge. Set `OTEL_EXPORTER_OTLP_ENDPOINT` to export OTLP/HTTP; without it, spans stay in-process.
