//! JSON ABI types for sandbox guests.

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

/// Legacy PrestaShop-style hook payload for the PHP migration guest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyHookInput {
    /// Hook name (for example `actionCartUpdateQuantityBefore`).
    pub hook: String,
    /// Host cart identifier.
    pub cart_id: String,
    /// Legacy product id (stringified).
    pub id_product: String,
    /// Requested quantity.
    pub quantity: u32,
    /// Operator (`up`, `down`, or `set`).
    pub operator: String,
}

/// Domain event draft emitted by a migration guest (host still commits).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainEventDraft {
    /// Stable event type the host recognizes.
    pub event_type: String,
    /// Cart identifier.
    pub cart_id: String,
    /// Product identifier mapped from the legacy hook.
    pub product_id: String,
    /// Proposed quantity.
    pub quantity: u32,
    /// Operator mirrored from the hook input.
    pub operator: String,
}
