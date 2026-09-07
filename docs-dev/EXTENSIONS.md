# Extensions (Component Model / WIT)

## Goal

Give rustashop a **Meteor-level opinion on extensibility**: modules attach to **stable interfaces**, not by overriding core types. The Wasm Component Model and WIT are the preferred ABI for first-party and third-party commerce plugins.

Implement the ABI **early and scoped** (one or two hooks + harness), not “rewrite the monolith as components.”

## Host and guest

| Side                   | Responsibility                                                                                                        |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------- |
| **Host (Rust kernel)** | Domain truth, persistence, payments capture, inventory commit, emitting realtime events, loading/verifying components |
| **Guest (component)**  | Pure or capability-limited logic: propose adjustments, quotes, classifications                                        |

Guests **cannot** open the database or talk to payment providers except through host functions the WIT world imports.

## Example hook surface (v0 candidates)

| Hook                        | Input (conceptual)            | Output                        |
| --------------------------- | ----------------------------- | ----------------------------- |
| `pricing-adjust`            | Cart snapshot (money as ints) | Line or order adjustments     |
| `shipping-quote`            | Address + parcels             | Quotes                        |
| `tax-rule`                  | Taxable breakdown             | Tax lines                     |
| `payment-webhook-normalize` | Raw provider payload          | Normalized domain event draft |

Start with **one** hook (`pricing-adjust`) plus a fixture component in CI.

### v0 layout (issue #35)

| Path | Role |
| --- | --- |
| [`extensions/wit/v0/world.wit`](../extensions/wit/v0/world.wit) | WIT world `pricing-adjust` (`export adjust`) |
| [`extensions/fixtures/pricing-adjust`](../extensions/fixtures/pricing-adjust) | Guest fixture + checked-in `pricing_adjust.component.wasm` |
| [`crates/rustashop-extensions`](../crates/rustashop-extensions) | Host invoke helper + unit tests |

Rebuild the fixture with `make extensions-fixture` (needs `wasm32-unknown-unknown` + `wasm-tools`). Host tests call `invoke_pricing_adjust` and assert deterministic discounts.

Isolation / denied-import tests and golden I/O live in
`crates/rustashop-extensions/tests/isolation.rs` ([#36](https://github.com/Interchouette-ITC/rustashop/issues/36)).
Engine ADR (wasmtime vs Wasmer) lands with the remaining epic outcomes on [#34](https://github.com/Interchouette-ITC/rustashop/issues/34).

## OpenAPI vs WIT

|           | OpenAPI                   | WIT                                             |
| --------- | ------------------------- | ----------------------------------------------- |
| Consumers | UIs, HTTP integrators     | Extension authors                               |
| Stability | Semver with the HTTP API  | Semver with the extension world                 |
| Evolution | Additive paths and fields | Additive functions / carefully versioned worlds |

Both describe the **same domain**. They are two doors, not two products.

## Hosting engines

Prefer a Component Model-capable host for plugins (wasmtime is the usual baseline today). Wasmer may participate where it supports the same WIT worlds or where a plugin is delivered as a Wasmer package; that choice is an ADR when the first harness lands. Do not block the ABI on picking every engine forever.

## Isolation tests (required habit)

Every extension hook gets a **module isolation test**:

1. Load guest with only declared imports.
2. Attempt forbidden host calls (persist, raw net) → denied.
3. Golden I/O for the hook fixture.
4. Optional: fuzz money fields stay integers / non-negative where required.

These tests are part of the extension epic acceptance, not a nice-to-have.

For `pricing-adjust`, see `cargo test -p rustashop-extensions --test isolation` (fixture has zero imports; empty linker denies a hostile `rustashop:forbidden/persist` import; golden discount cases).

## Relationship to Wasmer sandboxes

| Component Model plugin               | Wasmer sandbox script                            |
| ------------------------------------ | ------------------------------------------------ |
| Signed/versioned commerce extension  | Ad-hoc or merchant/agent code in Python/Node/PHP |
| Tight WIT world                      | Broader runtime + stricter outer jail            |
| Ships with the shop’s extension list | Jobs, migrations, playgrounds, agent tools       |

A future path may compile some sandboxed workflows _into_ components; early on, keep the APIs separate so trust levels stay clear.

## Native connectors (PyO3 and friends)

Some integrations want **in-process native bindings** (for example Rust host ↔ Python via PyO3) for low-latency connectors we ship ourselves. That is **not** the same as merchant-supplied Python in a sandbox:

|        | PyO3 (or similar) connector              | Wasmer Python guest               |
| ------ | ---------------------------------------- | --------------------------------- |
| Author | rustashop / trusted partner              | Merchant, agent, migrator         |
| Trust  | First-party                              | Untrusted                         |
| Use    | Official connectors, shared memory paths | Scripts, experiments, legacy glue |

Document connector work under extensions/integrations issues; enforce sandbox for anything executable that is not first-party.
