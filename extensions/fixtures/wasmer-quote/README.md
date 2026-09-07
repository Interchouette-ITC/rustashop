# wasmer-quote fixture

Fixed Python guest for the Wasmer polyglot sandbox harness (`rustashop-sandbox`).

## Contract

- **stdin:** JSON cart snapshot (`currency`, `lines[]` with `sku`, `quantity`, `unit_price`)
- **stdout:** JSON array of adjustments (`label`, `amount_minor`, `currency`)
- Host validates adjustments before any commerce apply

## Run (host)

```bash
cargo test -p rustashop-sandbox
```

The host loads this script into a Wasmer-hosted Python package (no Docker, no host CPython required for the crate test).
