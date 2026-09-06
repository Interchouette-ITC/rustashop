# rustashop

Commerce API image for **rustashop**: Actix HTTP (`rustashop-api`) plus embedded SQLx migrate binary (`rustashop-migrate`).

Source: [Interchouette-ITC/rustashop](https://github.com/Interchouette-ITC/rustashop)

## What’s inside

| Binary | Role |
| --- | --- |
| `rustashop-api` | Commerce HTTP API (default `CMD`) |
| `rustashop-migrate` | Apply embedded SQLx migrations (`DATABASE_URL`) |

Multi-stage build → distroless `cc-debian13` (Debian 13, non-root). Serenade package config is baked under `/app/config`.

## Quick start

```bash
docker pull interchouette/rustashop:dev

# API (needs Postgres via DATABASE_URL)
docker run --rm -p 8080:8080 \
  -e DATABASE_URL=postgres://user:pass@host:5432/rustashop \
  interchouette/rustashop:dev

# Migrations only
docker run --rm \
  -e DATABASE_URL=postgres://user:pass@host:5432/rustashop \
  interchouette/rustashop:dev /app/rustashop-migrate
```

## Tags

| Tag | Meaning |
| --- | --- |
| `:dev` | Latest development image (manual / on-demand push) |
| `:X.Y.Z` | Release matching workspace `Cargo.toml` / GitHub Release `vX.Y.Z` |
| `:latest` | Moves with each release |

## Docs

- Repo: [github.com/Interchouette-ITC/rustashop](https://github.com/Interchouette-ITC/rustashop)
- Docker details: [`docker/README.md`](https://github.com/Interchouette-ITC/rustashop/blob/dev/docker/README.md)
- Website: [interchouette.net](https://interchouette.net/)

## License

See the repository for license terms (OSL-3.0).
