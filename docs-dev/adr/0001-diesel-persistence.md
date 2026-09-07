# ADR 0001: Diesel persistence adapter

- **Status:** Accepted (defer)
- **Date:** 2026-09-07
- **Issue:** [#57](https://github.com/Interchouette-ITC/rustashop/issues/57)

## Context

Catalog and commerce persistence already ship two async adapters: **SQLx** (default) and **SeaORM** (feature-selected). Both implement the same domain repository traits against one Postgres schema. Issue #57 asked for a third-path **Diesel** spike to decide adopt / defer / drop.

Diesel 2.x remains primarily a sync ORM/query builder. Async use today means **`diesel-async`** (Tokio + `AsyncPgConnection` / pools) or `spawn_blocking` around a sync `PgConnection`. That is a third stack beside SQLx and SeaORM's sqlx runtime, with its own schema macros, migration story, and connection model.

## Decision

**Defer.** Keep an experimental `rustashop-persist-diesel` crate that proves `ProductRepository::find_by_id` on the existing `product` table. Do **not** wire Diesel into `rustashop-persist` features, default `make run-api`, or compose.

## Consequences

| Do | Do not |
| --- | --- |
| Build/test the spike with `cargo test -p rustashop-persist-diesel` | Select Diesel from the persist facade |
| Prefer SQLx or SeaORM for new repositories | Expand Diesel to cart/checkout/order parity |
| Revisit only if a concrete product need favors Diesel's query DSL | Put Diesel on the default image path |

## Spike notes

- Adapter: `DieselCatalogRepository` + `diesel-async` (>= 0.9) Postgres.
- Scope: one table (`product`), one real method (`find_by_id`); other `ProductRepository` methods return `PersistenceError::Internal` with an explicit spike message.
- Schema setup in tests reuses `rustashop-persist-sqlx::migrate` so Diesel does not own migrations.
