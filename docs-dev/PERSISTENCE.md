# Persistence SQL safety

Adapters in `rustashop-persist-sqlx` and `rustashop-persist-seaorm` use **parameterized** queries (SQLx binds) or the SeaORM query builder. That is what stops SQL injection. String concatenation must not build SQL from request or domain data.

The experimental `rustashop-persist-diesel` spike uses Diesel's query DSL the same way (bound parameters). It is **not** part of the default facade; see [ADR 0001](adr/0001-diesel-persistence.md).

## Layers (Symfony-style)

| Layer | Responsibility |
| --- | --- |
| **HTTP / request** (`rustashop-api` `request_param`) | Once at the edge: reject NUL in path, body, and header strings that come from the client |
| **Persistence adapters** | Binds / query builder only; no per-bind `ensure_param` spray |
| **`param::ensure_param`** | Shared helper kept for opt-in use (notably raw SQL fragments); same policy as contracts |
| **`RUSTASHOP_ALLOW_RAW_SQL`** | Gate for any client-supplied SQL text (default off) |

`ensure_param` is **not** a second injection firewall. Injection is prevented by never concatenating user data into SQL. The helper is input hygiene (NUL / interop), callable from a single upper layer or from the raw-SQL escape hatch.

## Raw client SQL

There is no public HTTP handler that accepts a client SQL string. Internal escape hatch:

| Item | Behavior |
| --- | --- |
| Env | `RUSTASHOP_ALLOW_RAW_SQL` (default unset / off) |
| Entry | `raw_sql::execute_fragment` in each persist crate |
| Flag off | `PersistenceError::InvalidInput` |
| Flag on | Loud stderr warning, then execute (still runs `ensure_param`) |

Static migrations and catalog seed scripts stay outside that gate.

## Lint

`make check-sql-safety` runs `cargo test -p rustashop-persist-sqlx --test sql_safety` (also from `make lint`). It fails if persist crate sources use `format!(…)` with SQL keywords to build statements.
