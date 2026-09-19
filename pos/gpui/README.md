# rustashop POS / TPV / caisse (GPUI)

Native **front-of-house** desktop client: catalog sync, encaissement, append-only
ticket journal, and clôture stub. Speaks the public Commerce API catalog.

Not a shop Tauri clone. Not rangular templates.

**This build is not NF525 / LNE certified.** The journal is a durable hash-chain
skeleton for future fiscal work; do not claim certification in product copy.

## Run

```bash
# API must be up (e.g. make run-api) with catalog seed
make pos-gpui
```

Overrides:

| Env / flag | Role |
| --- | --- |
| `RUSTASHOP_API_BASE` / `--api-base` | Actix base (default `http://127.0.0.1:8080`) |
| `RUSTASHOP_POS_JOURNAL` / `--journal` | JSONL journal path (default `pos/gpui/data/journal.jsonl`) |

On weak GPUs, set `WGPU_BACKEND=gl` before launch if the native GPU backend fails.

## Gates

```bash
make test-pos-gpui
make lint-pos-gpui
```

## Flow

1. **Sync catalog** - public `GET /v1/products` + detail (enabled SKUs with stock).
2. Add lines to the sale → **Pay exact** writes a ticket to the journal.
3. **Clôture** closes open tickets into a period summary record.
