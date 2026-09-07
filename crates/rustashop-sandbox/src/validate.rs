//! Host validation stub for guest-proposed adjustments.

use anyhow::{Result, bail};

use crate::types::{Adjustment, CartSnapshot};

/// Rejects adjustments the host must never apply.
///
/// # Errors
///
/// Returns an error when a label is empty or currency does not match the cart.
pub fn validate_adjustments(cart: &CartSnapshot, raw: &[Adjustment]) -> Result<()> {
    for adj in raw {
        if adj.label.trim().is_empty() {
            bail!("adjustment label must be non-empty");
        }
        if adj.currency != cart.currency {
            bail!(
                "adjustment currency `{}` does not match cart `{}`",
                adj.currency,
                cart.currency
            );
        }
    }
    Ok(())
}

/// Returns guest adjustments only after [`validate_adjustments`] succeeds.
///
/// The host never invents money: rejected proposals are dropped as an error.
///
/// # Errors
///
/// Propagates validation failures.
pub fn apply_validated_adjustments(
    cart: &CartSnapshot,
    raw: Vec<Adjustment>,
) -> Result<Vec<Adjustment>> {
    validate_adjustments(cart, &raw)?;
    Ok(raw)
}
