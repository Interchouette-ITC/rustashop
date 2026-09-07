//! Diesel catalog spike: `ProductRepository::find_by_id` only.

use crate::schema::product;
use diesel::prelude::*;
use diesel_async::AsyncConnection;
use diesel_async::RunQueryDsl;
use diesel_async::pg::AsyncPgConnection;
use rustashop_domain::{Product, ProductRepository};
use serenade_contracts::{PageRequest, PersistenceError};
use std::future::Future;
use std::str::FromStr;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Diesel catalog read adapter (spike; `find_by_id` only).
pub struct DieselCatalogRepository {
    conn: Mutex<AsyncPgConnection>,
}

impl std::fmt::Debug for DieselCatalogRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DieselCatalogRepository")
            .finish_non_exhaustive()
    }
}

impl DieselCatalogRepository {
    /// Wraps an established async Postgres connection.
    #[must_use]
    pub fn new(conn: AsyncPgConnection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    /// Connects with `diesel-async` using a Postgres URL.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError`] when the connection fails.
    pub async fn connect(database_url: &str) -> Result<Self, PersistenceError> {
        let conn = AsyncPgConnection::establish(database_url)
            .await
            .map_err(|error| PersistenceError::Internal {
                message: error.to_string(),
            })?;
        Ok(Self::new(conn))
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = product)]
struct ProductRow {
    id: Uuid,
    category_id: Option<Uuid>,
    slug: String,
    name: String,
    description: Option<String>,
    enabled: bool,
}

impl ProductRow {
    fn into_product(self) -> Product {
        Product {
            id: self.id.to_string(),
            category_id: self.category_id.map(|id| id.to_string()),
            slug: self.slug,
            name: self.name,
            description: self.description,
            enabled: self.enabled,
        }
    }
}

fn parse_uuid(id: &str) -> Result<Uuid, PersistenceError> {
    Uuid::from_str(id).map_err(|error| PersistenceError::InvalidInput {
        message: error.to_string(),
    })
}

fn spike_only(method: &str) -> PersistenceError {
    PersistenceError::Internal {
        message: format!("diesel spike implements find_by_id only ({method})"),
    }
}

impl ProductRepository for DieselCatalogRepository {
    type Error = PersistenceError;
    type Id = String;
    type Product = Product;

    async fn find_by_id(&self, id: &Self::Id) -> Result<Option<Self::Product>, Self::Error> {
        let uuid = parse_uuid(id)?;
        let mut conn = self.conn.lock().await;
        let row = product::table
            .find(uuid)
            .select(ProductRow::as_select())
            .first::<ProductRow>(&mut *conn)
            .await
            .optional()
            .map_err(|error| PersistenceError::Internal {
                message: error.to_string(),
            })?;
        drop(conn);
        Ok(row.map(ProductRow::into_product))
    }

    fn find_by_slug(
        &self,
        _slug: &str,
    ) -> impl Future<Output = Result<Option<Self::Product>, Self::Error>> + Send {
        std::future::ready(Err(spike_only("find_by_slug")))
    }

    fn list(
        &self,
        _page: PageRequest,
    ) -> impl Future<Output = Result<Vec<Self::Product>, Self::Error>> + Send {
        std::future::ready(Err(spike_only("list")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_product_repo<T: ProductRepository>() {}

    #[test]
    fn diesel_catalog_implements_product_repository() {
        assert_product_repo::<DieselCatalogRepository>();
    }
}
