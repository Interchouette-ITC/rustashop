# Domains and deploy surfaces

Product name: **rustashop**. DNS hostnames stay lowercase (`rustashop.ai`, …).

Operator creates Render services; **no Render Blueprint / service YAML is maintained in this repo** for that hand-off. This note is the product map so DNS, images, and clients stay aligned.

## Hostnames

| Host                          | Role                                                                |
| ----------------------------- | ------------------------------------------------------------------- |
| `rustashop.interchouette.net` | First **`:dev` image** deploy (API / stack tip while building)      |
| `rustashop.ai`                | **Primary** marketing and public brand                              |
| `rustashop.io`                | **Product**-oriented surface (docs, product home, API-facing story) |
| `rustashop.dev`               | **Demo** shop / playground                                          |
| `rustashop.app`               | **Ionic** (or mobile) app entry                                     |
| `rustashop.nl`                | Redirect → `rustashop.ai` (for now)                                 |
| `rustashop.eu`                | Redirect → `rustashop.ai` (for now)                                 |
| `rustashop.fr`                | Redirect → `rustashop.ai` (for now)                                 |

## Images

| Pull (preferred) | Role |
| --- | --- |
| `interchouette/rustashop:dev` | Tip / rolling API (+ migrate binary in the same image) |
| `interchouette/rustashop:X.Y.Z` | Release matching workspace `Cargo.toml` / GitHub Release `vX.Y.Z` |
| `ghcr.io/interchouette-itc/rustashop:dev` | Same tip on org GHCR |

Local compose continues to build `rustashop-api:local` via `make stack-up`. See [`docker/README.md`](../docker/README.md).

## Deploy order (intent)

1. Ship a **dev image** and attach it to `rustashop.interchouette.net` (Render service owned by the operator; pull `interchouette/rustashop:dev`).
2. Point marketing at `rustashop.ai`.
3. Stand up `rustashop.io` / `rustashop.dev` when product and demo builds exist.
4. `rustashop.app` follows the mobile client.
5. Keep `.nl` / `.eu` / `.fr` as redirects until localized sites are justified.

## Client mapping (later)

| Surface                             | Likely client                    |
| ----------------------------------- | -------------------------------- |
| Marketing (`ai`)                    | Static / Angular marketing       |
| Product (`io`)                      | Docs + product pages             |
| Demo (`dev`)                        | Full storefront against demo API |
| App (`app`)                         | Ionic                            |
| Tip (`interchouette.net` subdomain) | Dev API / preview                |

Do not invent Render service files here.
