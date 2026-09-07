# rustashop-sandbox

Wasmer polyglot sandbox host. First job: run the fixed Python `quote(cart) → adjustments` fixture under WASIX, then apply only after a host validation stub.

This lane is separate from WIT plugins in `rustashop-extensions`.

## Test

```bash
cargo test -p rustashop-sandbox
```

First run downloads the pinned Wasmer Python package into `.wasmer/` (gitignored). Override cache root with `RUSTASHOP_WASMER_CACHE`.
