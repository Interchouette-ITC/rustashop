//! Wasmer polyglot sandbox host for merchant/agent scripts.
//!
//! Lane is distinct from WIT Component Model plugins (`rustashop-extensions`).
//! Guests propose values; the host validates before any commerce apply.

#![forbid(unsafe_code)]

mod host;
mod types;
mod validate;

pub use host::{
    PYTHON_PACKAGE_URL, invoke_python_quote, invoke_rust_wasi_quote, quote_fixture_source,
    rust_quote_wasm_path,
};
pub use types::{Adjustment, CartLine, CartSnapshot, Money};
pub use validate::{apply_validated_adjustments, validate_adjustments};

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cart_volume() -> CartSnapshot {
        CartSnapshot {
            currency: "EUR".into(),
            lines: vec![CartLine {
                sku: "HOODIE-M".into(),
                quantity: 2,
                unit_price: Money {
                    amount_minor: 5000,
                    currency: "EUR".into(),
                },
            }],
        }
    }

    fn sample_cart_small() -> CartSnapshot {
        CartSnapshot {
            currency: "EUR".into(),
            lines: vec![CartLine {
                sku: "TEE-S".into(),
                quantity: 1,
                unit_price: Money {
                    amount_minor: 2500,
                    currency: "EUR".into(),
                },
            }],
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn python_quote_volume_discount_after_host_validation() {
        let cart = sample_cart_volume();
        let raw = invoke_python_quote(&cart, quote_fixture_source())
            .await
            .expect("wasmer python quote");
        let applied = apply_validated_adjustments(&cart, raw).expect("validate");
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].label, "volume-discount");
        assert_eq!(applied[0].amount_minor, -1000);
        assert_eq!(applied[0].currency, "EUR");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn python_quote_skips_small_carts() {
        let cart = sample_cart_small();
        let raw = invoke_python_quote(&cart, quote_fixture_source())
            .await
            .expect("wasmer python quote");
        let applied = apply_validated_adjustments(&cart, raw).expect("validate");
        assert_eq!(applied, []);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rust_wasi_quote_skips_small_carts() {
        let cart = sample_cart_small();
        let raw = invoke_rust_wasi_quote(&cart)
            .await
            .expect("rust wasi quote");
        let applied = apply_validated_adjustments(&cart, raw).expect("validate");
        assert_eq!(applied, []);
    }

    #[test]
    fn validation_rejects_currency_mismatch() {
        let cart = sample_cart_small();
        let bad = vec![Adjustment {
            label: "x".into(),
            amount_minor: -1,
            currency: "USD".into(),
        }];
        assert!(validate_adjustments(&cart, &bad).is_err());
    }

    #[test]
    fn validation_rejects_empty_label() {
        let cart = sample_cart_small();
        let bad = vec![Adjustment {
            label: String::new(),
            amount_minor: -1,
            currency: "EUR".into(),
        }];
        assert!(validate_adjustments(&cart, &bad).is_err());
    }
}
