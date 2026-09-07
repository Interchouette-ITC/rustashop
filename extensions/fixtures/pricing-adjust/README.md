# pricing-adjust fixture

Guest component for WIT world `pricing-adjust` (`extensions/wit/v0`).

Deterministic rule: line subtotal (minor units) `>= 10_000` → one
`volume-discount` adjustment of `-subtotal/10`.

## Rebuild

Requires `wasm32-unknown-unknown` and `wasm-tools` on PATH:

```bash
make extensions-fixture
```

Checked-in artifact: `pricing_adjust.component.wasm` (used by host tests).
