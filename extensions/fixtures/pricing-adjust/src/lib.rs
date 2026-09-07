//! Guest fixture for `pricing-adjust` (WIT world v0).
//!
//! Deterministic rule: carts whose line subtotal (minor units) is at least
//! `10_000` receive a 10% volume discount labeled `volume-discount`.

wit_bindgen::generate!({
    world: "pricing-adjust",
    path: "../../wit/v0",
});

struct Fixture;

impl Guest for Fixture {
    fn adjust(cart: CartSnapshot) -> Vec<Adjustment> {
        let subtotal: i64 = cart
            .lines
            .iter()
            .map(|line| i64::from(line.quantity).saturating_mul(line.unit_price.amount_minor))
            .sum();
        if subtotal >= 10_000 {
            vec![Adjustment {
                label: "volume-discount".into(),
                amount_minor: -(subtotal / 10),
            }]
        } else {
            Vec::new()
        }
    }
}

export!(Fixture);
