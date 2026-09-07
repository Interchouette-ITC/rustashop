//! JSON ABI types for sandbox `quote(cart) -> adjustments`.

use serde::{Deserialize, Serialize};

/// Money amount in minor units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    /// Integer minor units (for example cents).
    pub amount_minor: i64,
    /// ISO-like currency code.
    pub currency: String,
}

/// One cart line in the guest snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CartLine {
    /// SKU identifier.
    pub sku: String,
    /// Line quantity.
    pub quantity: u32,
    /// Unit price.
    pub unit_price: Money,
}

/// Cart snapshot passed to the guest on stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CartSnapshot {
    /// Cart currency.
    pub currency: String,
    /// Lines.
    pub lines: Vec<CartLine>,
}

/// Proposed adjustment returned by the guest on stdout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adjustment {
    /// Human-readable label.
    pub label: String,
    /// Signed minor units (negative = discount).
    pub amount_minor: i64,
    /// Must match the cart currency after host validation.
    pub currency: String,
}
