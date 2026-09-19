//! Money display helpers (API minor units).

use serde::{Deserialize, Serialize};

/// Money amount from the Commerce API (`amount_minor` + ISO currency).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Money {
    /// Integer minor units (cents for EUR).
    pub amount_minor: i64,
    /// ISO 4217 currency code.
    pub currency: String,
}

impl Money {
    /// Formats as `major.cents CURRENCY` (minor units ÷ 100).
    #[must_use]
    pub fn display(&self) -> String {
        let negative = self.amount_minor < 0;
        let abs = self.amount_minor.unsigned_abs();
        let major = abs / 100;
        let cents = abs % 100;
        if negative {
            format!("-{major}.{cents:02} {}", self.currency)
        } else {
            format!("{major}.{cents:02} {}", self.currency)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Money;

    #[test]
    fn display_splits_minor_units() {
        let m = Money {
            amount_minor: 1250,
            currency: "EUR".into(),
        };
        assert_eq!(m.display(), "12.50 EUR");
    }
}
