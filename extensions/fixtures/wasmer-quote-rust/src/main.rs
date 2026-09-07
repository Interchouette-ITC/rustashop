//! WASI quote guest: JSON cart on stdin → adjustments on stdout.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Money {
    amount_minor: i64,
}

#[derive(Debug, Deserialize)]
struct CartLine {
    quantity: u32,
    unit_price: Money,
}

#[derive(Debug, Deserialize)]
struct CartSnapshot {
    currency: String,
    lines: Vec<CartLine>,
}

#[derive(Debug, Serialize)]
struct Adjustment {
    label: String,
    amount_minor: i64,
    currency: String,
}

fn quote(cart: &CartSnapshot) -> Vec<Adjustment> {
    let mut adjustments = Vec::new();
    for line in &cart.lines {
        if line.quantity >= 2 {
            let amount_minor =
                -i64::from(line.quantity) * line.unit_price.amount_minor / 10;
            adjustments.push(Adjustment {
                label: "volume-discount".into(),
                amount_minor,
                currency: cart.currency.clone(),
            });
        }
    }
    adjustments
}

fn main() {
    let cart: CartSnapshot = serde_json::from_reader(std::io::stdin().lock())
        .expect("parse cart JSON from stdin");
    let adjustments = quote(&cart);
    serde_json::to_writer(std::io::stdout().lock(), &adjustments)
        .expect("write adjustments JSON");
}
