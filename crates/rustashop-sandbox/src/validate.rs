//! Host validation stubs for guest-proposed values.

use anyhow::{Result, bail};

use crate::types::{Adjustment, CartSnapshot, DomainEventDraft};

/// Supported migration hook → domain event type.
pub const CART_UPDATE_QUANTITY_HOOK: &str = "actionCartUpdateQuantityBefore";
/// Domain event type drafted from [`CART_UPDATE_QUANTITY_HOOK`].
pub const CART_LINE_QUANTITY_PROPOSED: &str = "cart.line_quantity_proposed";

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

/// Rejects migration event drafts the host must never accept.
///
/// # Errors
///
/// Returns an error when required fields are empty, the event type is unknown,
/// or the operator is not one of `up` / `down` / `set`.
pub fn validate_domain_event_draft(draft: &DomainEventDraft) -> Result<()> {
    if draft.event_type != CART_LINE_QUANTITY_PROPOSED {
        bail!("unsupported domain event type `{}`", draft.event_type);
    }
    if draft.cart_id.trim().is_empty() {
        bail!("domain event cart_id must be non-empty");
    }
    if draft.product_id.trim().is_empty() {
        bail!("domain event product_id must be non-empty");
    }
    match draft.operator.as_str() {
        "up" | "down" | "set" => Ok(()),
        other => bail!("unsupported operator `{other}`"),
    }
}

/// Returns the draft only after [`validate_domain_event_draft`] succeeds.
///
/// # Errors
///
/// Propagates validation failures.
pub fn accept_validated_domain_event(draft: DomainEventDraft) -> Result<DomainEventDraft> {
    validate_domain_event_draft(&draft)?;
    Ok(draft)
}
