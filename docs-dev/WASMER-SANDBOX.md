# Wasmer SDK and polyglot sandboxes

## Why this axis exists

The [Wasmer SDK local sandboxes](https://wasmer.io/posts/wasmer-local-sandboxes-for-ai-agents) model embeds runtimes (Python, Node/Edge.js, PHP, Postgres-as-package, and more) **inside** the host application as Wasm-isolated guests: no Docker daemon, fast create, usable from Rust/JS/Python hosts and, for demos, the browser.

For rustashop this is an **innovation and safety surface**: polyglot execution wrapped around a Rust commerce kernel. It is a separate foundation axis from storefront Wasm and from WIT plugins ([WASM-LAYERS.md](WASM-LAYERS.md)).

## What we are buying

| Capability         | Product meaning                                                                 |
| ------------------ | ------------------------------------------------------------------------------- |
| In-process sandbox | Run untrusted or foreign code next to the API without a remote microVM per call |
| Polyglot packages  | Accept merchant skills in languages they already use                            |
| Agent-local tools  | Admin/agent codegen runs locally under our audit policy                         |
| Density            | Many short-lived sandboxes per host vs container sprawl                         |
| Browser twin       | Same teaching ABI in docs/playgrounds via Wasmer JS                             |

## High-value uses

### 1. Plugin / script sandbox

Capability-scoped jobs, for example `quote(cart) → adjustments`, implemented in:

- **Python** - pricing experiments, ranking, merchant data science
- **Node** - CMS/PIM transformers, npm ecosystem glue
- **PHP** - legacy PrestaShop/Sylius-era snippets during migration without running PHP-FPM as the shop
- **Rust→Wasm** - our own high-assurance samples on the same job API

Host remains Rust. Guests see snapshots and return proposals; host applies or rejects.

### 2. Agent-empowered admin

Default: executable artifacts from the assistant run **in a sandbox**:

- report scripts
- migration dry-runs against a **scratch** embedded DB package (not production)
- stdout/stderr streamed to the admin UI over the realtime gateway

### 3. Browser playground

Wasmer JS + Angular or Leptos shell:

- teach extensions without a cloud tenant
- “paste a pricing function, see cart update” onboarding
- marketing demos of polyglot safety

### 4. Migration and compatibility

Reference PHP trees under local `PHP/` (gitexcluded) inform adapters. Product angle: **execute** carefully wrapped legacy glue during cutover, emit rustashop domain events. We are not “a PHP host”; we are a **migration runtime** when needed.

### 5. Composed peripheral runtimes

One deployable kernel may run approved Node/Python/PHP packages for peripheral work (CMS fetch, forecast, tax table script) while money capture stays in Rust.

### 6. Scratch analytics / SQL pad

Embedded Postgres-as-package (or similar) for merchant scratchpads, agent exploration of exports, CI without Docker-in-Docker. Production checkout persistence stays on the host transactional store.

### 7. Edge / offline appliance

Rust binary + Wasmer packages for POS or air-gapped plugins that cannot call a cloud sandbox service on every tool call.

## UI roles (drivers, not engines)

| UI                    | Role                                                                                          |
| --------------------- | --------------------------------------------------------------------------------------------- |
| **Angular admin**     | Script console, extension IDE, agent panel: create job, stream logs, apply or discard results |
| **rangular / Leptos** | Storefront; optional in-browser Wasmer demo mode for docs                                     |
| **Both**              | Consume host APIs such as job create/status and WS job events                                 |

## Host-mediated rules

Sandboxes must not:

- Capture cards or finalize capture/void alone
- Commit inventory alone
- Receive unscoped filesystem or network by default

Rust authorizes; Wasm executes within a jail; audit logs are mandatory.

## In tree today

| Piece | Location |
| --- | --- |
| Host crate | [`crates/rustashop-sandbox`](../crates/rustashop-sandbox) (Wasmer WASIX + pinned `python/python@0.1.0`) |
| Python fixture | [`extensions/fixtures/wasmer-quote/quote.py`](../extensions/fixtures/wasmer-quote/quote.py) |
| Rust WASI fixture | [`extensions/fixtures/wasmer-quote-rust/`](../extensions/fixtures/wasmer-quote-rust/) (`quote.wasm`, rebuild with `make sandbox-quote-rust-fixture`) |
| JS fixture | [`extensions/fixtures/wasmer-quote/quote.js`](../extensions/fixtures/wasmer-quote/quote.js) (pinned `syrusakbary/quickjs`) |
| PHP fixture | [`extensions/fixtures/wasmer-quote/quote.php`](../extensions/fixtures/wasmer-quote/quote.php) (pinned `php/php-32`) |
| PHP migration guest | [`extensions/fixtures/wasmer-php-migration/`](../extensions/fixtures/wasmer-php-migration/) (`actionCartUpdateQuantityBefore` → `cart.line_quantity_proposed`) |
| Cache | `.wasmer/` (gitignored); override with `RUSTASHOP_WASMER_CACHE` |

```bash
cargo test -p rustashop-sandbox
```

Upstream `wasmer-sdk` (unpublished package-first facade) is not a Cargo dependency yet; this crate uses the published Wasmer WASIX runner APIs as the equivalent host surface.

### Migration guest limits

The PHP migration fixture proves cutover glue only:

- One PrestaShop-inspired hook family (`actionCartUpdateQuantityBefore`)
- Guest returns a **domain event draft**; the Rust host validates and remains the authority that commits cart state
- No PHP-FPM, no legacy DB writes, no network from the guest by default
- Not a general PrestaShop runtime; richer hooks stay later slices under #37 / #40

## Suggested delivery slices

| Slice                   | Outcome                                                                                      |
| ----------------------- | -------------------------------------------------------------------------------------------- |
| **Sandbox harness**     | Rust host creates a Wasmer sandbox, runs a fixed Python `quote` fixture, returns adjustments |
| **Admin console**       | Angular UI + audit log + WS logs for one job type                                            |
| **PHP migration guest** | One legacy hook family bridged to domain events                                              |
| **Browser twin**        | Same fixture ABI in Wasmer JS playground                                                     |
| **Agent default**       | Documented policy: agent tools execute sandboxed unless marked first-party                   |

## PyO3 and native connectors

Official connectors may later use **PyO3** (or similar) for trusted in-process Python. That path coexists with Wasmer:

- **Wasmer** - untrusted / merchant / agent / legacy scripts (default for agent tools unless marked first-party)
- **PyO3** - first-party connector performance and shared types

Do not expose PyO3 as the way merchants upload arbitrary `.py` files.

Policy ADR: [0003](adr/0003-pyo3-vs-wasmer-sandbox.md).

## References

- Wasmer blog: [Wasmer SDK: Local Sandboxes for AI Agents](https://wasmer.io/posts/wasmer-local-sandboxes-for-ai-agents)
- SDK repo (upstream): https://github.com/wasmerio/wasmer-sdk
