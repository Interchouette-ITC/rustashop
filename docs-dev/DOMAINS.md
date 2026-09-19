# Domains and deploy surfaces

Product name: **rustashop**. DNS hostnames stay lowercase (`rustashop.ai`, …).

Operator creates Render (or other) services; **no Render Blueprint / service YAML is maintained in this repo** for that hand-off. This note is the product map so DNS, images, and clients stay aligned.

## Scratch tip (not a product domain)

| Host | Role |
| --- | --- |
| `rustashop.interchouette.net` | **Temporary trial** under Interchouette infra. Smoke-test a `:dev` image / stack **before** standing up the real rustashop domains. Not marketing, not demo, not the long-term tip hostname. |

Do not treat this subdomain as the product launch surface. When the real domains are live, this host may go away or stay as an internal scratch only.

## Product hostnames

| Host                          | Role                                                                |
| ----------------------------- | ------------------------------------------------------------------- |
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

The `:dev` image is what you try on the scratch tip first; the same image (or a release tag) later attaches to product hosts when those DNS / services exist.

## Deploy order (intent)

1. **Optional scratch:** publish `:dev` and try it on `rustashop.interchouette.net` (operator-owned; throwaway).
2. Stand up **product** DNS and services: marketing on `rustashop.ai`, then `rustashop.io` / `rustashop.dev` when product and demo builds exist.
3. `rustashop.app` follows the mobile client.
4. Keep `.nl` / `.eu` / `.fr` as redirects until localized sites are justified.

## Client mapping (later)

| Surface | Likely client |
| --- | --- |
| Marketing (`ai`) | Static / Angular marketing |
| Product (`io`) | Docs + product pages |
| Demo (`dev`) | Full storefront against demo API |
| App (`app`) | Ionic |
| Scratch (`interchouette.net` subdomain) | Temporary API / stack trial only |

Do not invent Render service files here.
