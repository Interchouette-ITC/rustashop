# rustashop-sandbox

Wasmer polyglot sandbox host. Runs fixed guest fixtures (Python / Rust WASI / QuickJS / PHP quote, plus a PHP migration hook) under WASIX, then applies results only after host validation.

This lane is separate from WIT plugins in `rustashop-extensions`.

## Test

```bash
cargo test -p rustashop-sandbox
```

First run downloads pinned Wasmer packages into `.wasmer/` (gitignored). Override cache root with `RUSTASHOP_WASMER_CACHE`.
