# wasmer-quote fixture

Fixed polyglot guests for the Wasmer sandbox harness (`rustashop-sandbox`).

## Contract

- **stdin:** JSON cart snapshot (`currency`, `lines[]` with `sku`, `quantity`, `unit_price`)
- **stdout:** JSON array of adjustments (`label`, `amount_minor`, `currency`)
- Host validates adjustments before any commerce apply

## Guests

| File | Host entry | Wasmer package |
| --- | --- | --- |
| `quote.py` | `invoke_python_quote` | `python/python@0.1.0` |
| `quote.js` | `invoke_js_quote` | `syrusakbary/quickjs` (`qjs --std -e`) |
| `quote.php` | `invoke_php_quote` | `php/php-32` (`php -r`) |

Rust WASI twin lives in sibling [`wasmer-quote-rust/`](../wasmer-quote-rust/).

## Run (host)

```bash
cargo test -p rustashop-sandbox
```

Packages cache under `.wasmer/` (no Docker).
