# rustashop-persist

Compile-time persistence facade. Enable **exactly one** backend:

| Feature | Backend |
| --- | --- |
| `persist-sqlx` (default) | `rustashop-persist-sqlx` |
| `persist-seaorm` | `rustashop-persist-seaorm` |

Enabling both features fails at compile time. Downstream crates depend on this
facade, not on a concrete adapter.
