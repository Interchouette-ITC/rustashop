//! Integer minor-unit money.

use serde::{Deserialize, Serialize};

use crate::DomainError;

/// ISO 4217 alphabetic currency code (exactly three ASCII letters).
///
/// # Examples
///
/// ```
/// use rustashop_domain::Currency;
///
/// let eur = Currency::new("eur").expect("valid");
/// assert_eq!(eur.as_str(), "EUR");
/// assert!(Currency::new("eu").is_err());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Currency(String);

impl Currency {
    /// Parses a three-letter currency code (ASCII case-insensitive).
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidCurrency`] when the code is not three letters.
    pub fn new(code: impl Into<String>) -> Result<Self, DomainError> {
        let code = code.into().to_ascii_uppercase();
        if code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic()) {
            Ok(Self(code))
        } else {
            Err(DomainError::InvalidCurrency(code))
        }
    }

    /// Currency code as uppercase text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Formats as `{major}.{frac:02} {currency}` assuming two decimal minor units
/// (cents). Matches EUR/USD-style currencies used by the storefront.
impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let negative = self.amount_minor < 0;
        let abs = self.amount_minor.unsigned_abs();
        let major = abs / 100;
        let frac = abs % 100;
        if negative {
            write!(f, "-{major}.{frac:02} {}", self.currency)
        } else {
            write!(f, "{major}.{frac:02} {}", self.currency)
        }
    }
}

/// Money amount in minor units for a single currency.
///
/// # Examples
///
/// ```
/// use rustashop_domain::{Currency, Money};
///
/// let eur = Currency::new("EUR").expect("valid");
/// let left = Money::new(199, eur.clone());
/// let right = Money::new(50, eur);
/// assert_eq!(left.checked_add(&right).expect("add").amount_minor, 249);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    /// Amount in the currency's minor unit (for example cents).
    pub amount_minor: i64,
    /// ISO currency for `amount_minor`.
    pub currency: Currency,
}

impl Money {
    /// Creates money in `currency` without changing the minor-unit scale.
    #[must_use]
    pub const fn new(amount_minor: i64, currency: Currency) -> Self {
        Self {
            amount_minor,
            currency,
        }
    }

    /// Adds two amounts of the same currency.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::CurrencyMismatch`] or [`DomainError::Overflow`].
    pub fn checked_add(&self, other: &Self) -> Result<Self, DomainError> {
        self.ensure_same_currency(other)?;
        let amount_minor = self
            .amount_minor
            .checked_add(other.amount_minor)
            .ok_or(DomainError::Overflow)?;
        Ok(Self::new(amount_minor, self.currency.clone()))
    }

    /// Subtracts `other` from this amount.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::CurrencyMismatch`] or [`DomainError::Overflow`].
    pub fn checked_sub(&self, other: &Self) -> Result<Self, DomainError> {
        self.ensure_same_currency(other)?;
        let amount_minor = self
            .amount_minor
            .checked_sub(other.amount_minor)
            .ok_or(DomainError::Overflow)?;
        Ok(Self::new(amount_minor, self.currency.clone()))
    }

    /// Multiplies the minor-unit amount by a positive quantity.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidQuantity`] when `quantity` is not positive,
    /// or [`DomainError::Overflow`] when the product does not fit in `i64`.
    pub fn checked_mul_qty(&self, quantity: i32) -> Result<Self, DomainError> {
        if quantity <= 0 {
            return Err(DomainError::InvalidQuantity(quantity));
        }
        let amount_minor = self
            .amount_minor
            .checked_mul(i64::from(quantity))
            .ok_or(DomainError::Overflow)?;
        Ok(Self::new(amount_minor, self.currency.clone()))
    }

    fn ensure_same_currency(&self, other: &Self) -> Result<(), DomainError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(DomainError::CurrencyMismatch {
                left: self.currency.as_str().to_owned(),
                right: other.currency.as_str().to_owned(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sync collaborator double for domain ports (mockall + `cargo test`).
    #[mockall::automock]
    trait CurrencyCodeSource {
        fn code(&self) -> &'static str;
    }

    #[test]
    fn mockall_currency_code_source_builds_currency() {
        let mut source = MockCurrencyCodeSource::new();
        source.expect_code().return_const("eur");
        let currency = Currency::new(source.code()).expect("valid");
        assert_eq!(currency.as_str(), "EUR");
    }

    #[rstest::rstest]
    #[case::lower("eur", "EUR")]
    #[case::upper("USD", "USD")]
    #[case::mixed("gBp", "GBP")]
    fn currency_normalizes_ascii_case(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(Currency::new(input).unwrap().as_str(), expected);
    }

    #[rstest::rstest]
    #[case::too_short("eu")]
    #[case::digit("eu1")]
    fn currency_rejects_invalid_codes(#[case] input: &str) {
        assert!(matches!(
            Currency::new(input),
            Err(DomainError::InvalidCurrency(_))
        ));
    }

    #[test]
    fn add_and_sub_same_currency() {
        let eur = Currency::new("EUR").unwrap();
        let left = Money::new(199, eur.clone());
        let right = Money::new(50, eur);
        assert_eq!(left.checked_add(&right).unwrap().amount_minor, 249);
        assert_eq!(left.checked_sub(&right).unwrap().amount_minor, 149);
    }

    #[test]
    fn currency_mismatch_is_error() {
        let left = Money::new(100, Currency::new("EUR").unwrap());
        let right = Money::new(100, Currency::new("USD").unwrap());
        assert!(matches!(
            left.checked_add(&right),
            Err(DomainError::CurrencyMismatch { .. })
        ));
    }

    #[test]
    fn overflow_does_not_panic() {
        let eur = Currency::new("EUR").unwrap();
        let left = Money::new(i64::MAX, eur.clone());
        let right = Money::new(1, eur);
        assert_eq!(left.checked_add(&right), Err(DomainError::Overflow));
        assert_eq!(
            Money::new(i64::MIN, Currency::new("EUR").unwrap()).checked_sub(&right),
            Err(DomainError::Overflow)
        );
    }

    #[test]
    fn mul_qty_rejects_zero_and_overflow() {
        let eur = Currency::new("EUR").unwrap();
        let money = Money::new(100, eur.clone());
        assert_eq!(money.checked_mul_qty(3).unwrap().amount_minor, 300);
        assert_eq!(
            money.checked_mul_qty(0),
            Err(DomainError::InvalidQuantity(0))
        );
        assert_eq!(
            Money::new(i64::MAX, eur).checked_mul_qty(2),
            Err(DomainError::Overflow)
        );
    }

    #[test]
    fn money_display_uses_two_decimal_minor_units() {
        let eur = Currency::new("EUR").unwrap();
        assert_eq!(Money::new(199, eur.clone()).to_string(), "1.99 EUR");
        assert_eq!(Money::new(50, eur.clone()).to_string(), "0.50 EUR");
        assert_eq!(Money::new(0, eur.clone()).to_string(), "0.00 EUR");
        assert_eq!(Money::new(-125, eur).to_string(), "-1.25 EUR");
    }
}
