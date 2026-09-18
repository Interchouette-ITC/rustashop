//! Last order snapshot for the confirmation route (router state analog).

use leptos::prelude::*;

use crate::api::Order;

/// Holds the order returned by the last successful checkout in this tab.
#[derive(Clone, Copy)]
pub struct OrderCtx {
    /// Last placed order, if any.
    pub last: RwSignal<Option<Order>>,
}

impl OrderCtx {
    /// Creates and provides [`OrderCtx`].
    pub fn provide() -> Self {
        let ctx = Self {
            last: RwSignal::new(None),
        };
        provide_context(ctx);
        ctx
    }
}

/// Returns the provided [`OrderCtx`].
///
/// # Panics
///
/// Panics when `OrderCtx` was not provided.
#[must_use]
pub fn use_order() -> OrderCtx {
    use_context::<OrderCtx>().expect("OrderCtx provided")
}
