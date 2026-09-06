# Docker image (API + migrate)

Size-optimized multi-stage build → `gcr.io/distroless/cc-debian13:nonroot` (Debian 13 / trixie family; no apt, perl, or shell).

| Item | Value |
| --- | --- |
| Binaries | `rustashop-api`, `rustashop-migrate` |
| Default CMD | `/app/rustashop-api` |
| Builder | `rust:slim-trixie` |
| Config | `/app/config` (Serenade packages); `RUSTASHOP_ROOT=/app` |
| Bind | `RUSTASHOP_BIND` default `0.0.0.0:8080` |
| Local compose | `docker/compose.yml` builds `rustashop-api:local` |

## Where to pull images (public)

| Registry | Image |
| --- | --- |
| Docker Hub | `interchouette/rustashop` |
| GHCR | `ghcr.io/interchouette-itc/rustashop` |
| GHCR | `ghcr.io/interchouette/rustashop` |

Public docs advertise Hub `interchouette/rustashop` and org GHCR `ghcr.io/interchouette-itc/rustashop`. Personal GHCR `ghcr.io/groussac/rustashop` is a Make/CI mirror only.

```bash
docker pull interchouette/rustashop:dev
docker pull ghcr.io/interchouette-itc/rustashop:dev
```

## Tags

| Tag | Who pushes | When |
| --- | --- | --- |
| `:dev` | Local `make` or Actions `workflow_dispatch` (“CI/CD Image dev”) | On demand |
| `:X.Y.Z` | GitHub Actions on **Release** | Tag `vX.Y.Z` must match workspace `Cargo.toml` |
| `:latest` | Same release workflow | Moves with each release |

## Make targets

```bash
make docker-build          # Hub :latest + :$(APP_VERSION)
make docker-build-dev      # :dev on Hub + three GHCR names
make docker-push-dev       # local interactive logins
make docker-push-release   # :version + :latest (local; CI uses split targets)
```

## Local stack

`make stack-up` still **builds** the Dockerfile and tags `rustashop-api:local`. It does not pull the published Hub image.

## Environment

| Env | Meaning |
| --- | --- |
| `DATABASE_URL` | Postgres DSN (required for API and migrate) |
| `RUSTASHOP_BIND` | Listen address (default `0.0.0.0:8080`) |
| `RUSTASHOP_ROOT` | Shop root for `config/packages` (default `/app` in the image) |
| `RUSTASHOP_ADMIN_API_TOKEN` | Admin bearer when enabling admin routes |
| `RUSTASHOP_ADMIN_API_PREFIX` | Opaque admin API URI segment |

## Publishing secrets (GitHub Actions)

| Secret | Use |
| --- | --- |
| `DOCKER_USERNAME` / `DOCKER_PASSWORD` | Docker Hub (`interchouette`) |
| `GHCR_USERNAME` / `GHCR_PAT` | Personal GHCR |
| `GHCR_USERNAME_ITC` / `GHCR_PAT_ITC` | Worker + org GHCR |

Hub Overview text is synced from [`DOCKERHUB.md`](DOCKERHUB.md).
