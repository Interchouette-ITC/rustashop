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

## Logging

Process logging goes through `serenade-observability` (`LoggingConfig` + `init`). Default sinks: stderr and rolling files under `var/log/` (stem from `RUSTASHOP_ENV`, default `dev`). Filter: `SERENADE_LOG` or `RUST_LOG`.

Optional feature `otel` on `rustashop-api` enables the OpenTelemetry bridge. Set `OTEL_EXPORTER_OTLP_ENDPOINT` to export OTLP/HTTP; without it, spans stay in-process.
