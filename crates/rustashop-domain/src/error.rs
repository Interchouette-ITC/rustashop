//! Domain error types.

/// Failure in money or cart domain rules.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// Currency code is not a three-letter ISO alphabetic code.
    #[error("invalid currency code `{0}` (expected three ASCII letters)")]
    InvalidCurrency(String),
    /// Arithmetic mixed two different currencies.
    #[error("currency mismatch: cannot combine `{left}` and `{right}`")]
    CurrencyMismatch {
        /// Left-hand currency code.
        left: String,
        /// Right-hand currency code.
        right: String,
    },
    /// Checked arithmetic overflowed `i64`.
    #[error("money amount overflowed i64 minor units")]
    Overflow,
    /// Quantity must be a positive integer.
    #[error("invalid quantity `{0}` (must be a positive integer)")]
    InvalidQuantity(i32),
    /// Cart line id is not present on the cart.
    #[error("cart line `{0}` was not found")]
    LineNotFound(String),
    /// Checkout requires at least one line.
    #[error("cart is empty; add at least one line before checkout")]
    EmptyCart,
    /// Cart was already converted to an order.
    #[error("cart was already checked out")]
    CartAlreadyCheckedOut,
    /// Stored cart status is not `open` or `checked_out`.
    #[error("invalid cart status `{0}` (expected `open` or `checked_out`)")]
    InvalidCartStatus(String),
    /// Stored or requested order state is not a known fulfillment value.
    #[error("invalid order state `{0}` (expected `placed`, `paid`, `shipped`, or `cancelled`)")]
    InvalidOrderState(String),
    /// Requested order status jump is not allowed by the fulfillment workflow.
    #[error("illegal order transition from `{from}` to `{to}`")]
    IllegalOrderTransition {
        /// Current fulfillment place.
        from: String,
        /// Requested fulfillment place.
        to: String,
    },
    /// Product slug input produced an empty URL slug after normalization.
    #[error("invalid product slug `{0}` (empty after normalization)")]
    InvalidProductSlug(String),
}
