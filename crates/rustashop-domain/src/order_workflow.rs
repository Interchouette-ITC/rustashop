//! Order fulfillment transitions via `serenade-workflow`.

use std::sync::{Arc, OnceLock};

use serenade_workflow::{
    Definition, DefinitionBuilder, Marking, MarkingStore, MemoryMarkingStore, Workflow,
};

use crate::{DomainError, OrderState};

/// Shared order fulfillment definition (`placed` → `paid` → `shipped`, plus cancel edges).
///
/// # Panics
///
/// Panics only if the static definition graph is inconsistent (compile-time constant edges).
#[must_use]
pub fn order_definition() -> &'static Definition {
    static DEFINITION: OnceLock<Definition> = OnceLock::new();
    DEFINITION.get_or_init(|| {
        DefinitionBuilder::new()
            .places(["placed", "paid", "shipped", "cancelled"])
            .initial(Marking::single("placed"))
            .edge("mark_paid", "placed", "paid")
            .edge("ship", "paid", "shipped")
            .edge("cancel_from_placed", "placed", "cancelled")
            .edge("cancel_from_paid", "paid", "cancelled")
            .build()
            .expect("order fulfillment workflow definition")
    })
}

/// Validates that moving from `from` to `to` is an enabled workflow transition.
///
/// Same-state updates are allowed (idempotent admin PATCH).
///
/// # Errors
///
/// Returns [`DomainError::IllegalOrderTransition`] when the jump is not enabled.
pub fn assert_order_transition(from: OrderState, to: OrderState) -> Result<(), DomainError> {
    if from == to {
        return Ok(());
    }
    let store = Arc::new(MemoryMarkingStore::new());
    store.set("order", Marking::single(from.as_str()));
    let workflow = Workflow::new("order", order_definition().clone(), store);
    let target = to.as_str();
    let allowed = order_definition().transitions().iter().any(|candidate| {
        workflow.can("order", candidate.name())
            && candidate.to().iter().any(|place| place == target)
    });
    if allowed {
        Ok(())
    } else {
        Err(illegal(from, to))
    }
}

fn illegal(from: OrderState, to: OrderState) -> DomainError {
    DomainError::IllegalOrderTransition {
        from: from.as_str().to_owned(),
        to: to.as_str().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_happy_path_and_cancel() {
        assert_order_transition(OrderState::Placed, OrderState::Paid).unwrap();
        assert_order_transition(OrderState::Paid, OrderState::Shipped).unwrap();
        assert_order_transition(OrderState::Placed, OrderState::Cancelled).unwrap();
        assert_order_transition(OrderState::Paid, OrderState::Cancelled).unwrap();
        assert_order_transition(OrderState::Paid, OrderState::Paid).unwrap();
    }

    #[test]
    fn rejects_illegal_jumps() {
        assert!(matches!(
            assert_order_transition(OrderState::Placed, OrderState::Shipped),
            Err(DomainError::IllegalOrderTransition { .. })
        ));
        assert!(matches!(
            assert_order_transition(OrderState::Shipped, OrderState::Paid),
            Err(DomainError::IllegalOrderTransition { .. })
        ));
        assert!(matches!(
            assert_order_transition(OrderState::Cancelled, OrderState::Paid),
            Err(DomainError::IllegalOrderTransition { .. })
        ));
        assert!(matches!(
            assert_order_transition(OrderState::Shipped, OrderState::Cancelled),
            Err(DomainError::IllegalOrderTransition { .. })
        ));
    }

    #[test]
    fn order_definition_exposes_places() {
        let places = order_definition().places();
        assert!(places.iter().any(|place| place == "placed"));
        assert!(places.iter().any(|place| place == "shipped"));
    }
}
