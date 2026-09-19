//! In-memory sale (cart) for the POS window.

use crate::catalog::SellableSku;
use crate::journal::TicketLine;
use crate::money::Money;

/// One line on the active sale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaleLine {
    /// Variant id.
    pub variant_id: String,
    /// SKU.
    pub sku: String,
    /// Product display name.
    pub product_name: String,
    /// Quantity.
    pub quantity: i32,
    /// Unit price.
    pub unit_price: Money,
}

impl SaleLine {
    /// Line total in minor units.
    #[must_use]
    pub fn line_total_minor(&self) -> i64 {
        i64::from(self.quantity) * self.unit_price.amount_minor
    }
}

/// Active sale being built at the register.
#[derive(Clone, Debug, Default)]
pub struct Sale {
    lines: Vec<SaleLine>,
}

impl Sale {
    /// Adds one unit of a sellable SKU (or increments quantity).
    pub fn add_sku(&mut self, sku: &SellableSku) {
        if let Some(line) = self
            .lines
            .iter_mut()
            .find(|l| l.variant_id == sku.variant_id)
        {
            line.quantity = line.quantity.saturating_add(1);
            return;
        }
        self.lines.push(SaleLine {
            variant_id: sku.variant_id.clone(),
            sku: sku.sku.clone(),
            product_name: sku.product_name.clone(),
            quantity: 1,
            unit_price: sku.unit_price.clone(),
        });
    }

    /// Clears all lines.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Returns sale lines.
    #[must_use]
    pub fn lines(&self) -> &[SaleLine] {
        &self.lines
    }

    /// Whether the sale has lines.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Currency of the first line, or `EUR` when empty.
    #[must_use]
    pub fn currency(&self) -> &str {
        self.lines
            .first()
            .map_or("EUR", |l| l.unit_price.currency.as_str())
    }

    /// Sum of line totals in minor units.
    #[must_use]
    pub fn total_minor(&self) -> i64 {
        self.lines.iter().map(SaleLine::line_total_minor).sum()
    }

    /// Converts to journal ticket lines.
    #[must_use]
    pub fn to_ticket_lines(&self) -> Vec<TicketLine> {
        self.lines
            .iter()
            .map(|l| TicketLine {
                variant_id: l.variant_id.clone(),
                sku: l.sku.clone(),
                product_name: l.product_name.clone(),
                quantity: l.quantity,
                unit_price_minor: l.unit_price.amount_minor,
                line_total_minor: l.line_total_minor(),
                currency: l.unit_price.currency.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sku(id: &str, price: i64) -> SellableSku {
        SellableSku {
            product_id: "p1".into(),
            product_name: "Hoodie".into(),
            variant_id: id.into(),
            sku: "SKU-1".into(),
            unit_price: Money {
                amount_minor: price,
                currency: "EUR".into(),
            },
            stock_quantity: 5,
        }
    }

    #[test]
    fn add_increments_same_variant() {
        let mut sale = Sale::default();
        sale.add_sku(&sku("v1", 1000));
        sale.add_sku(&sku("v1", 1000));
        assert_eq!(sale.lines().len(), 1);
        assert_eq!(sale.lines()[0].quantity, 2);
        assert_eq!(sale.total_minor(), 2000);
    }
}
