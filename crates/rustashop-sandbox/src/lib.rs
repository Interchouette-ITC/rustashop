//! Wasmer polyglot sandbox host for merchant/agent scripts.
//!
//! Lane is distinct from WIT Component Model plugins (`rustashop-extensions`).
//! Guests propose values; the host validates before any commerce apply.

#![forbid(unsafe_code)]

mod host;
mod types;
mod validate;

pub use host::{
    JS_PACKAGE_URL, PHP_PACKAGE_URL, PYTHON_PACKAGE_URL, invoke_js_quote,
    invoke_php_migration_hook, invoke_php_quote, invoke_python_quote, invoke_rust_wasi_quote,
    php_migration_hook_source, quote_fixture_source, quote_js_fixture_source,
    quote_php_fixture_source, rust_quote_wasm_path,
};
pub use types::{Adjustment, CartLine, CartSnapshot, DomainEventDraft, LegacyHookInput, Money};
pub use validate::{
    CART_LINE_QUANTITY_PROPOSED, CART_UPDATE_QUANTITY_HOOK, accept_validated_domain_event,
    apply_validated_adjustments, validate_adjustments, validate_domain_event_draft,
};

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

    #[tokio::test(flavor = "multi_thread")]
    async fn js_quote_volume_discount_after_host_validation() {
        let cart = sample_cart_volume();
        let raw = invoke_js_quote(&cart, quote_js_fixture_source())
            .await
            .expect("wasmer js quote");
        let applied = apply_validated_adjustments(&cart, raw).expect("validate");
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].label, "volume-discount");
        assert_eq!(applied[0].amount_minor, -1000);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn js_quote_skips_small_carts() {
        let cart = sample_cart_small();
        let raw = invoke_js_quote(&cart, quote_js_fixture_source())
            .await
            .expect("wasmer js quote");
        let applied = apply_validated_adjustments(&cart, raw).expect("validate");
        assert_eq!(applied, []);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn php_quote_volume_discount_after_host_validation() {
        let cart = sample_cart_volume();
        let raw = invoke_php_quote(&cart, &quote_php_fixture_source())
            .await
            .expect("wasmer php quote");
        let applied = apply_validated_adjustments(&cart, raw).expect("validate");
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].label, "volume-discount");
        assert_eq!(applied[0].amount_minor, -1000);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn php_quote_skips_small_carts() {
        let cart = sample_cart_small();
        let raw = invoke_php_quote(&cart, &quote_php_fixture_source())
            .await
            .expect("wasmer php quote");
        let applied = apply_validated_adjustments(&cart, raw).expect("validate");
        assert_eq!(applied, []);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn php_migration_cart_update_hook_emits_domain_event_draft() {
        let input = LegacyHookInput {
            hook: CART_UPDATE_QUANTITY_HOOK.into(),
            cart_id: "cart-1".into(),
            id_product: "42".into(),
            quantity: 3,
            operator: "up".into(),
        };
        let draft = invoke_php_migration_hook(&input, &php_migration_hook_source())
            .await
            .expect("php migration guest");
        let accepted = accept_validated_domain_event(draft).expect("validate draft");
        assert_eq!(accepted.event_type, CART_LINE_QUANTITY_PROPOSED);
        assert_eq!(accepted.cart_id, "cart-1");
        assert_eq!(accepted.product_id, "42");
        assert_eq!(accepted.quantity, 3);
        assert_eq!(accepted.operator, "up");
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

    #[test]
    fn domain_event_draft_validation_rejects_bad_fields() {
        let ok = DomainEventDraft {
            event_type: CART_LINE_QUANTITY_PROPOSED.into(),
            cart_id: "cart-1".into(),
            product_id: "42".into(),
            quantity: 1,
            operator: "up".into(),
        };
        assert!(accept_validated_domain_event(ok.clone()).is_ok());

        let mut bad = ok.clone();
        bad.event_type = "unknown".into();
        assert!(validate_domain_event_draft(&bad).is_err());

        bad = ok.clone();
        bad.cart_id = String::new();
        assert!(validate_domain_event_draft(&bad).is_err());

        bad = ok.clone();
        bad.product_id = "  ".into();
        assert!(validate_domain_event_draft(&bad).is_err());

        bad = ok;
        bad.operator = "sideways".into();
        assert!(validate_domain_event_draft(&bad).is_err());
    }
}
