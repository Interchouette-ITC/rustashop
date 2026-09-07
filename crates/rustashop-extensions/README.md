# rustashop-extensions

Host-side helpers for WIT Component Model extension worlds. v0 loads the
`pricing-adjust` fixture and calls `adjust`.

WIT: [`extensions/wit/v0/world.wit`](../../extensions/wit/v0/world.wit).  
Fixture: [`extensions/fixtures/pricing-adjust`](../../extensions/fixtures/pricing-adjust).

```bash
make extensions-fixture   # rebuild guest .wasm when WIT/guest changes
cargo test -p rustashop-extensions
cargo test -p rustashop-extensions --test isolation
```
