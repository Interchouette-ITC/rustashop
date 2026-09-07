# Contributing to rustashop

Thank you for improving rustashop. This repo is the **commerce product**. Framework work belongs in [Serenade](https://github.com/Interchouette-ITC/Serenade).

## Before you open a PR

1. Read [`ARCHITECTURE.md`](ARCHITECTURE.md) and [`docs-dev/`](../docs-dev/).
2. Run:

```bash
make lint
make test
```

Integration tests need Postgres (`make db-up`). Prefer Docker for the database.

Coverage (no %-fail gate in CI; reports upload to Codecov):

```bash
make db-up
make coverage      # Rust lcov → coverage/lcov.info (needs cargo-llvm-cov)
make coverage-js   # Vitest lcov for shop, admin, install
```

3. One concern per PR. Finish the concern locally, then open a **ready** PR (use draft only when the branch must be visible before that concern is done).
4. English only in code, docs, commits, and PR text. In markdown prose, write
   **Serenade** (capital S); crate ids stay lowercase (`serenade-contracts`).

## Toolchain

- Rust stable (see `rust-version` in the workspace `Cargo.toml`).
- Integration branch: `dev` on the org repo.
- Feature branches land via PR from the worker fork.

## Make targets

| Target | Purpose |
| --- | --- |
| `make lint` | `fmt --check` + clippy (`-D warnings`, pedantic, nursery) for workspace and SeaORM features |
| `make test` | workspace tests, then SeaORM feature tests |
| `make coverage` | Rust `cargo llvm-cov` → `coverage/lcov.info` |
| `make coverage-js` | Vitest coverage for shop, admin, install |
| `make openapi` | write `openapi/openapi.json` from utoipa |
| `make openapi-check` | regenerate OpenAPI and fail if the committed dump drifted |
| `make doc` | rustdoc (`-D warnings`) |
| `make shop-angular` | serve Angular shop (`shops/angular`, port 4242) |
| `make admin-angular` | serve Angular admin (`admin/angular`, port 4250) |
| `make shop-leptos-rangular` | serve Leptos+rangular shop (`shops/leptos-rangular`, port 4181) |
| `make run-api` | Actix API on host (`RUSTASHOP_BIND`, default `127.0.0.1:8080`) |
| `make db-up` | Postgres only via compose |
| `make stack-up` | Postgres + migrate + API image |
| `make db-migrate` | SQLx migrations |
| `make db-migrate-seaorm` | SeaORM migrations |
| `make db-seed` | catalog seed SQL (idempotent; does not wipe) |
| `make db-reset` | **DESTROYS** schema `public` then migrates; requires `CONFIRM=YES` |

Shared shop markup/SCSS: `templates/shop/default/`. Admin markup/SCSS: `templates/admin/default/`.
Kinds (`shop` | `admin`) are separate trees; see [`../templates/README.md`](../templates/README.md).
Do not edit generated adapters under `shops/*/generated/` or `admin/*/generated/`
(build output, gitignored).

Default DSN: `postgres://rustashop:rustashop@127.0.0.1:5432/rustashop`.

## Quality bar

Do not add `#[allow(clippy::too_many_arguments)]`, `too_many_lines`, or `dead_code`. Fix with structs, helpers, or by wiring/removing unused items.

Before opening or updating a PR, run the full local gate (`make ci`: lint, test, doc, openapi-check, audit, deny). Integration tests need Postgres (`make db-up`).

## Rust test DX

Runner stays **`cargo test`** (via `make test` / `make ci`). Prefer these workspace `dev-dependencies` when they fit:

| Crate | Use for |
| --- | --- |
| **rstest** | Parametrized cases and fixtures |
| **mockall** | Sync trait doubles when a real collaborator is heavy |
| **insta** | Stable JSON / text snapshots (`*.snap` committed; `*.snap.new` gitignored) |

HTTP integration style stays in `crates/rustashop-api/tests/` (Actix `test`).

## Persistence features

Default build uses `persist-sqlx`. SeaORM path:

```bash
cargo check -p rustashop-persist -p rustashop-api --no-default-features --features persist-seaorm
```

Experimental Diesel spike (not wired into the persist facade; see ADR 0001):

```bash
cargo test -p rustashop-persist-diesel
```

`make lint` and `make test` already cover both.

## Documentation

- **Product architecture:** [`ARCHITECTURE.md`](ARCHITECTURE.md)
- **Foundations** (Wasm, realtime, AI, domains): [`docs-dev/`](../docs-dev/)
- **OpenAPI:** live at `/openapi.json` and `/swagger-ui/`; committed dump via `make openapi` (CI runs `make openapi-check`)
- **Code of Conduct:** [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md)
- **Security policy:** [`SECURITY.md`](SECURITY.md)
- No plan jargon or host-absolute paths in shipped text

## Issues and epics

- Use GitHub issue forms (Bug report / Feature request) when opening issues.
- Commerce and product ops: this repo’s GitHub issues / milestones.
- Framework kernel, DI, HTTP foundation, console: Serenade issues.
- Kernel wire into rustashop: [#49](https://github.com/Interchouette-ITC/rustashop/issues/49).

## Commits and PRs

Conventional commits (`feat:`, `fix:`, `docs:`, `ci:`, …). PR body follows
[`pull_request_template.md`](pull_request_template.md) (**Summary** + **Test plan** only).

## License

This repository is licensed under **OSL-3.0** (see [`../LICENSE`](../LICENSE)).

## Questions

Open a GitHub issue on `Interchouette-ITC/rustashop` for product design. Framework questions that affect multiple apps go to Serenade.
