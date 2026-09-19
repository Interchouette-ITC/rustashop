//! rustashop GPUI POS library (catalog, sale, journal).

#![forbid(unsafe_code)]

pub mod catalog;
pub mod journal;
pub mod money;
pub mod sale;

pub use catalog::{CatalogClient, Config, ProductDetail, ProductVariant, SellableSku};
pub use journal::{ClosureSummary, Journal, JournalError, TicketLine, TicketRecord};
pub use money::Money;
pub use sale::{Sale, SaleLine};
