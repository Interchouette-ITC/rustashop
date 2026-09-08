//! Host helpers for rustashop WIT extension worlds (Component Model).

#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use serenade_component_host::{default_engine, empty_linker, load_component, store_with_data};
use std::path::Path;
use wasmtime::component::Linker;

#[allow(missing_docs)]
mod bindings {
    wasmtime::component::bindgen!({
        path: "../../extensions/wit/v0",
        world: "pricing-adjust",
    });
}

use bindings::PricingAdjust;
pub use bindings::{Adjustment, CartLine, CartSnapshot, Money};

/// Runs `adjust` on a `pricing-adjust` component at `component_path`.
///
/// # Errors
///
/// Returns an error when the component cannot be loaded or the guest call fails.
pub fn invoke_pricing_adjust(
    component_path: impl AsRef<Path>,
    cart: &CartSnapshot,
) -> Result<Vec<Adjustment>> {
    let engine = default_engine();
    let component = load_component(&engine, component_path.as_ref())
        .with_context(|| format!("load component {}", component_path.as_ref().display()))?;
    let linker: Linker<()> = empty_linker(&engine);
    let mut store = store_with_data(&engine, ());
    let instance = PricingAdjust::instantiate(&mut store, &component, &linker)
        .context("instantiate pricing-adjust")?;
    instance
        .call_adjust(&mut store, cart)
        .context("call adjust")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_component() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../extensions/fixtures/pricing-adjust/pricing_adjust.component.wasm")
    }

    #[test]
    fn abi_reexports_are_constructible() {
        let money = Money {
            amount_minor: 1,
            currency: "EUR".into(),
        };
        let line = CartLine {
            sku: "SKU".into(),
            quantity: 1,
            unit_price: money.clone(),
        };
        let cart = CartSnapshot {
            currency: "EUR".into(),
            lines: vec![line],
        };
        let adjustment = Adjustment {
            label: "x".into(),
            amount_minor: -1,
        };
        assert_eq!(cart.lines.len(), 1);
        assert_eq!(adjustment.amount_minor, -1);
        assert_eq!(money.currency, "EUR");
    }

    #[test]
    fn pricing_adjust_applies_volume_discount() {
        let cart = CartSnapshot {
            currency: "EUR".into(),
            lines: vec![CartLine {
                sku: "HOODIE-M".into(),
                quantity: 2,
                unit_price: Money {
                    amount_minor: 5000,
                    currency: "EUR".into(),
                },
            }],
        };
        let adjustments = invoke_pricing_adjust(fixture_component(), &cart).expect("invoke");
        assert_eq!(adjustments.len(), 1);
        assert_eq!(adjustments[0].label, "volume-discount");
        assert_eq!(adjustments[0].amount_minor, -1000);
    }

    #[test]
    fn pricing_adjust_skips_small_carts() {
        let cart = CartSnapshot {
            currency: "EUR".into(),
            lines: vec![CartLine {
                sku: "TEE-S".into(),
                quantity: 1,
                unit_price: Money {
                    amount_minor: 2500,
                    currency: "EUR".into(),
                },
            }],
        };
        let adjustments = invoke_pricing_adjust(fixture_component(), &cart).expect("invoke");
        assert!(adjustments.is_empty());
    }
}
