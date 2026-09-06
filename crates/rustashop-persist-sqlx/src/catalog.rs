//! `SQLx` catalog repositories.

use rustashop_domain::{Category, CategoryRepository, Product, ProductRepository};
use serenade_contracts::{PageRequest, PersistenceError};
use sqlx::postgres::PgPool;
use sqlx::FromRow;

/// `SQLx` catalog read adapter.
#[derive(Clone, Debug)]
pub struct SqlxCatalogRepository {
    pub(crate) pool: PgPool,
}

impl SqlxCatalogRepository {
    /// Creates a repository bound to `pool`.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Lists all products for admin (includes disabled), slug order.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError`] on query failure.
    pub async fn list_all_products(
        &self,
        page: PageRequest,
    ) -> Result<Vec<Product>, PersistenceError> {
        let rows = sqlx::query_as::<_, ProductRow>(
            "SELECT id::text AS id, category_id::text AS category_id, slug, name, description, enabled
             FROM product ORDER BY slug LIMIT $1 OFFSET $2",
        )
        .bind(i64::from(page.limit))
        .bind(i64::from(page.offset))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        Ok(rows.into_iter().map(ProductRow::into_product).collect())
    }

    /// Lists purchasable variants for a product (SKU order).
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError`] on query failure.
    pub async fn list_variants_for_product(
        &self,
        product_id: &str,
    ) -> Result<Vec<rustashop_domain::ProductVariant>, PersistenceError> {
        #[derive(Debug, FromRow)]
        struct VariantRow {
            id: String,
            product_id: String,
            sku: String,
            name: Option<String>,
            price_minor: i64,
            currency: String,
            stock_quantity: i32,
        }

        let rows = sqlx::query_as::<_, VariantRow>(
            "SELECT id::text AS id, product_id::text AS product_id, sku, name,
                    price_minor, currency, stock_quantity
             FROM product_variant
             WHERE product_id = $1::uuid
             ORDER BY sku",
        )
        .bind(product_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| internal(&error))?;

        rows.into_iter()
            .map(|row| {
                let currency = rustashop_domain::Currency::new(&row.currency).map_err(|error| {
                    PersistenceError::Internal {
                        message: error.to_string(),
                    }
                })?;
                Ok(rustashop_domain::ProductVariant {
                    id: row.id,
                    product_id: row.product_id,
                    sku: row.sku,
                    name: row.name,
                    price: rustashop_domain::Money::new(row.price_minor, currency),
                    stock_quantity: row.stock_quantity,
                })
            })
            .collect()
    }
}

#[derive(Debug, FromRow)]
struct ProductRow {
    id: String,
    category_id: Option<String>,
    slug: String,
    name: String,
    description: Option<String>,
    enabled: bool,
}

#[derive(Debug, FromRow)]
struct CategoryRow {
    id: String,
    parent_id: Option<String>,
    slug: String,
    name: String,
}

impl ProductRow {
    fn into_product(self) -> Product {
        Product {
            id: self.id,
            category_id: self.category_id,
            slug: self.slug,
            name: self.name,
            description: self.description,
            enabled: self.enabled,
        }
    }
}

impl CategoryRow {
    fn into_category(self) -> Category {
        Category {
            id: self.id,
            parent_id: self.parent_id,
            slug: self.slug,
            name: self.name,
        }
    }
}

fn internal(error: &sqlx::Error) -> PersistenceError {
    PersistenceError::Internal {
        message: error.to_string(),
    }
}

impl ProductRepository for SqlxCatalogRepository {
    type Error = PersistenceError;
    type Id = String;
    type Product = Product;

    async fn find_by_id(&self, id: &Self::Id) -> Result<Option<Self::Product>, Self::Error> {
        let row = sqlx::query_as::<_, ProductRow>(
            "SELECT id::text AS id, category_id::text AS category_id, slug, name, description, enabled
             FROM product WHERE id = $1::uuid",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        Ok(row.map(ProductRow::into_product))
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Self::Product>, Self::Error> {
        let row = sqlx::query_as::<_, ProductRow>(
            "SELECT id::text AS id, category_id::text AS category_id, slug, name, description, enabled
             FROM product WHERE slug = $1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        Ok(row.map(ProductRow::into_product))
    }

    async fn list(&self, page: PageRequest) -> Result<Vec<Self::Product>, Self::Error> {
        let rows = sqlx::query_as::<_, ProductRow>(
            "SELECT id::text AS id, category_id::text AS category_id, slug, name, description, enabled
             FROM product WHERE enabled = TRUE ORDER BY slug LIMIT $1 OFFSET $2",
        )
        .bind(i64::from(page.limit))
        .bind(i64::from(page.offset))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        Ok(rows.into_iter().map(ProductRow::into_product).collect())
    }
}

impl CategoryRepository for SqlxCatalogRepository {
    type Error = PersistenceError;
    type Id = String;
    type Category = Category;

    async fn find_by_id(&self, id: &Self::Id) -> Result<Option<Self::Category>, Self::Error> {
        let row = sqlx::query_as::<_, CategoryRow>(
            "SELECT id::text AS id, parent_id::text AS parent_id, slug, name
             FROM category WHERE id = $1::uuid",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        Ok(row.map(CategoryRow::into_category))
    }

    async fn find_by_slug(
        &self,
        slug: &str,
        parent_id: Option<&Self::Id>,
    ) -> Result<Option<Self::Category>, Self::Error> {
        let row = sqlx::query_as::<_, CategoryRow>(
            "SELECT id::text AS id, parent_id::text AS parent_id, slug, name
             FROM category
             WHERE slug = $1 AND parent_id IS NOT DISTINCT FROM $2::uuid",
        )
        .bind(slug)
        .bind(parent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        Ok(row.map(CategoryRow::into_category))
    }

    async fn list_children(
        &self,
        parent_id: Option<&Self::Id>,
        page: PageRequest,
    ) -> Result<Vec<Self::Category>, Self::Error> {
        let rows = sqlx::query_as::<_, CategoryRow>(
            "SELECT id::text AS id, parent_id::text AS parent_id, slug, name
             FROM category
             WHERE parent_id IS NOT DISTINCT FROM $1::uuid
             ORDER BY slug LIMIT $2 OFFSET $3",
        )
        .bind(parent_id)
        .bind(i64::from(page.limit))
        .bind(i64::from(page.offset))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        Ok(rows.into_iter().map(CategoryRow::into_category).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustashop_domain::{CategoryRepository, ProductRepository};

    fn assert_product_repo<T: ProductRepository>() {}
    fn assert_category_repo<T: CategoryRepository>() {}

    #[test]
    fn sqlx_catalog_implements_repository_traits() {
        assert_product_repo::<SqlxCatalogRepository>();
        assert_category_repo::<SqlxCatalogRepository>();
    }
}
