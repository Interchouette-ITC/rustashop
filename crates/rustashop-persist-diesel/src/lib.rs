//! Experimental Diesel persistence adapter (not part of the default facade).
//!
//! Implements [`ProductRepository::find_by_id`](rustashop_domain::ProductRepository::find_by_id)
//! against the shared `product` table. See `docs-dev/adr/0001-diesel-persistence.md`.

#![forbid(unsafe_code)]

mod catalog;
mod schema;

pub use catalog::DieselCatalogRepository;
