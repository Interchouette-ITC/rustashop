//! rustashop GPUI ops client library (HTTP + money helpers).

#![forbid(unsafe_code)]

pub mod api;
pub mod money;

pub use api::{
    next_order_status, AdminClient, Config, Order, ORDER_STATUSES, Product, ProductDetail,
    ProductVariant, Tab,
};
pub use money::Money;
