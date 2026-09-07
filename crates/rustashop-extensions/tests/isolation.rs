//! Extension module isolation tests (issue #36).

use rustashop_extensions::{CartLine, CartSnapshot, Money, invoke_pricing_adjust};
use std::path::PathBuf;
use wasmtime::component::{Component, Linker};
use wasmtime::{Engine, Store};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../extensions/fixtures/pricing-adjust/pricing_adjust.component.wasm")
}

fn golden_cart(sku: &str, quantity: u32, amount_minor: i64) -> CartSnapshot {
    CartSnapshot {
        currency: "EUR".into(),
        lines: vec![CartLine {
            sku: sku.into(),
            quantity,
            unit_price: Money {
                amount_minor,
                currency: "EUR".into(),
            },
        }],
    }
}

#[test]
fn fixture_loads_with_only_declared_imports() {
    let engine = Engine::default();
    let component = Component::from_file(&engine, fixture_path()).expect("load fixture");
    let imports: Vec<_> = component
        .component_type()
        .imports(&engine)
        .map(|(name, _)| name.to_owned())
        .collect();
    assert!(
        imports.is_empty(),
        "pricing-adjust fixture must declare no imports, got {imports:?}"
    );
    let cart = golden_cart("PROBE", 1, 1);
    invoke_pricing_adjust(fixture_path(), &cart).expect("instantiate via declared world only");
}

#[test]
fn forbidden_persist_import_fails_to_instantiate() {
    let engine = Engine::default();
    // Hostile guest: requires a host persist import the pricing linker never provides.
    let component = Component::new(
        &engine,
        r#"
        (component
          (import "rustashop:forbidden/persist" (instance
            (export "exec" (func (param "sql" string)))
          ))
        )
        "#,
    )
    .expect("parse hostile component");
    let imports: Vec<_> = component
        .component_type()
        .imports(&engine)
        .map(|(name, _)| name.to_owned())
        .collect();
    assert!(
        imports.iter().any(|name| name.contains("forbidden")),
        "expected forbidden import, got {imports:?}"
    );
    let linker = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let err = linker
        .instantiate(&mut store, &component)
        .expect_err("empty linker must deny undeclared persist import");
    let message = format!("{err:#}");
    assert!(
        message.contains("forbidden") || message.contains("import") || message.contains("unknown"),
        "unexpected deny message: {message}"
    );
}

#[test]
fn golden_pricing_adjust_io() {
    struct Case {
        name: &'static str,
        cart: CartSnapshot,
        expect_labels: &'static [&'static str],
        expect_amounts: &'static [i64],
    }

    let cases = [
        Case {
            name: "below-threshold",
            cart: golden_cart("TEE-S", 1, 2500),
            expect_labels: &[],
            expect_amounts: &[],
        },
        Case {
            name: "exact-threshold",
            cart: golden_cart("HOODIE-M", 2, 5000),
            expect_labels: &["volume-discount"],
            expect_amounts: &[-1000],
        },
        Case {
            name: "above-threshold",
            cart: golden_cart("BUNDLE", 3, 4000),
            expect_labels: &["volume-discount"],
            expect_amounts: &[-1200],
        },
    ];

    for case in &cases {
        let got = invoke_pricing_adjust(fixture_path(), &case.cart).unwrap_or_else(|error| {
            panic!("{}: invoke failed: {error:#}", case.name);
        });
        assert_eq!(
            got.len(),
            case.expect_labels.len(),
            "{}: adjustment count",
            case.name
        );
        for (i, adj) in got.iter().enumerate() {
            assert_eq!(
                adj.label, case.expect_labels[i],
                "{}: label[{i}]",
                case.name
            );
            assert_eq!(
                adj.amount_minor, case.expect_amounts[i],
                "{}: amount_minor[{i}]",
                case.name
            );
        }
    }
}

#[test]
fn missing_component_path_is_denied() {
    let cart = golden_cart("X", 1, 1);
    let err = invoke_pricing_adjust("/no/such/pricing_adjust.component.wasm", &cart)
        .expect_err("missing path");
    let message = format!("{err:#}");
    assert!(
        message.contains("load component"),
        "unexpected error: {message}"
    );
}
