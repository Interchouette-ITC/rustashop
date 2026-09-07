# rustashop-persist-diesel

Experimental Diesel + `diesel-async` spike for catalog `ProductRepository::find_by_id`
against the existing `product` table.

Not selected by `rustashop-persist` features. Default API and compose stay on SQLx
(or SeaORM). See [`docs-dev/adr/0001-diesel-persistence.md`](../../docs-dev/adr/0001-diesel-persistence.md).

```bash
cargo test -p rustashop-persist-diesel
# with Postgres:
DATABASE_URL=postgres://rustashop:rustashop@127.0.0.1:5432/rustashop \
  cargo test -p rustashop-persist-diesel --test find_by_id
```
