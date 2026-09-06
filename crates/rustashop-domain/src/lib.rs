//! Commerce domain types. Persistence adapters live outside this crate.
//!
//! Money uses integer minor units and an ISO currency code, matching Sylius order
//! totals (`getTotal(): int`) rather than float cart math.

mod cart;
mod catalog;
mod error;
mod money;
mod order;
mod repositories;

pub use cart::{Cart, CartLine, CartStatus};
pub use catalog::{Category, Product, ProductVariant};
pub use error::DomainError;
pub use money::{Currency, Money};
pub use order::{Order, OrderLine, OrderState, PAYMENT_STATUS_PENDING};
pub use repositories::{CartRepository, CategoryRepository, ProductRepository};
