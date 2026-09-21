//! Commerce domain types. Persistence adapters live outside this crate.
//!
//! Money uses integer minor units and an ISO currency code, matching Sylius order
//! totals (`getTotal(): int`) rather than float cart math.

mod ai_tools;
mod cart;
mod catalog;
mod error;
mod money;
mod order;
mod order_workflow;
mod product_slug;
mod repositories;

pub use ai_tools::{
    TOOLS, ToolDescriptor, ToolEffect, ToolScope, tool_by_name, tools_catalog_json,
};
pub use cart::{Cart, CartLine, CartStatus};
pub use catalog::{Category, Product, ProductVariant};
pub use error::DomainError;
pub use money::{Currency, Money};
pub use order::{Order, OrderLine, OrderState, PAYMENT_STATUS_PENDING};
pub use order_workflow::{assert_order_transition, order_definition};
pub use product_slug::product_slug;
pub use repositories::{CartRepository, CategoryRepository, ProductRepository};
