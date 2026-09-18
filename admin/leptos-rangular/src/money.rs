//! Money display helpers (API minor units).

use serde::Deserialize;

/// Money amount from the Commerce API (`amount_minor` + ISO currency).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Money {
    pub amount_minor: i64,
    pub currency: String,
}

impl Money {
    /// Formats as `major.cents CURRENCY` (minor units ÷ 100).
    #[must_use]
    pub fn display(&self) -> String {
        let major = self.amount_minor / 100;
        let cents = self.amount_minor.rem_euclid(100);
        format!("{major}.{cents:02} {}", self.currency)
    }
}

#[cfg(test)]
mod tests {
    use super::Money;

    #[test]
    fn display_splits_minor_units() {
        let m = Money {
            amount_minor: 4500,
            currency: "EUR".into(),
        };
        assert_eq!(m.display(), "45.00 EUR");
        let m = Money {
            amount_minor: 99,
            currency: "EUR".into(),
        };
        assert_eq!(m.display(), "0.99 EUR");
    }
}
