//! Commerce repository ports. Adapters live in persist crates.

use serenade_contracts::{EntityId, PageRequest, RepositoryError};
use std::future::Future;

/// Catalog product reads. Product writes live beside admin use-cases.
pub trait ProductRepository: Send + Sync {
    /// Error type for this adapter.
    type Error: RepositoryError;
    /// Product identifier type (for example UUID string).
    type Id: EntityId;
    /// Product aggregate or view model.
    type Product: Send + Sync;

    /// Load one product by primary key.
    fn find_by_id(
        &self,
        id: &Self::Id,
    ) -> impl Future<Output = Result<Option<Self::Product>, Self::Error>> + Send;

    /// Load one product by URL slug.
    fn find_by_slug(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<Option<Self::Product>, Self::Error>> + Send;

    /// List products for storefront browse.
    fn list(
        &self,
        page: PageRequest,
    ) -> impl Future<Output = Result<Vec<Self::Product>, Self::Error>> + Send;
}

/// Category tree reads for navigation and catalog filters.
pub trait CategoryRepository: Send + Sync {
    /// Error type for this adapter.
    type Error: RepositoryError;
    /// Category identifier type.
    type Id: EntityId;
    /// Category model.
    type Category: Send + Sync;

    /// Load one category by primary key.
    fn find_by_id(
        &self,
        id: &Self::Id,
    ) -> impl Future<Output = Result<Option<Self::Category>, Self::Error>> + Send;

    /// Load one category by slug within an optional parent scope.
    fn find_by_slug(
        &self,
        slug: &str,
        parent_id: Option<&Self::Id>,
    ) -> impl Future<Output = Result<Option<Self::Category>, Self::Error>> + Send;

    /// List child categories for a parent (`None` = roots).
    fn list_children(
        &self,
        parent_id: Option<&Self::Id>,
        page: PageRequest,
    ) -> impl Future<Output = Result<Vec<Self::Category>, Self::Error>> + Send;
}

/// Cart session persistence. Line mutations snapshot price in the domain layer.
pub trait CartRepository: Send + Sync {
    /// Error type for this adapter.
    type Error: RepositoryError;
    /// Cart identifier type.
    type Id: EntityId;
    /// Cart aggregate.
    type Cart: Send + Sync;

    /// Resolve a cart by opaque session token.
    fn find_by_token(
        &self,
        token: &str,
    ) -> impl Future<Output = Result<Option<Self::Cart>, Self::Error>> + Send;

    /// Insert or update a cart aggregate.
    fn save(&self, cart: &Self::Cart) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Remove a cart when checkout completes or session expires.
    fn delete(&self, id: &Self::Id) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
