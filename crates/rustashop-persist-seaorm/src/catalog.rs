//! `SeaORM` catalog repositories.

use rustashop_domain::{Category, CategoryRepository, Product, ProductRepository};
use sea_orm::entity::prelude::*;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serenade_contracts::{PageRequest, PersistenceError};

use crate::entities::{category, product};

/// `SeaORM` catalog read adapter.
#[derive(Clone, Debug)]
pub struct SeaOrmCatalogRepository {
    pub(crate) db: DatabaseConnection,
}

impl SeaOrmCatalogRepository {
    /// Creates a repository bound to `db`.
    #[must_use]
    pub const fn new(db: DatabaseConnection) -> Self {
        Self { db }
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
        let rows = product::Entity::find()
            .order_by_asc(product::Column::Slug)
            .limit(u64::from(page.limit))
            .offset(u64::from(page.offset))
            .all(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        Ok(rows.into_iter().map(product_from_model).collect())
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
        use crate::entities::product_variant;

        let uuid = parse_uuid(product_id)?;
        let rows = product_variant::Entity::find()
            .filter(product_variant::Column::ProductId.eq(uuid))
            .order_by_asc(product_variant::Column::Sku)
            .all(&self.db)
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
                    id: row.id.to_string(),
                    product_id: row.product_id.to_string(),
                    sku: row.sku,
                    name: row.name,
                    price: rustashop_domain::Money::new(row.price_minor, currency),
                    stock_quantity: row.stock_quantity,
                })
            })
            .collect()
    }
}

fn internal(error: &DbErr) -> PersistenceError {
    PersistenceError::Internal {
        message: error.to_string(),
    }
}

fn parse_uuid(id: &str) -> Result<Uuid, PersistenceError> {
    Uuid::parse_str(id).map_err(|_| PersistenceError::InvalidInput {
        message: format!("invalid id `{id}`"),
    })
}

fn product_from_model(model: product::Model) -> Product {
    Product {
        id: model.id.to_string(),
        category_id: model.category_id.map(|id| id.to_string()),
        slug: model.slug,
        name: model.name,
        description: model.description,
        enabled: model.enabled,
    }
}

fn category_from_model(model: category::Model) -> Category {
    Category {
        id: model.id.to_string(),
        parent_id: model.parent_id.map(|id| id.to_string()),
        slug: model.slug,
        name: model.name,
    }
}

impl ProductRepository for SeaOrmCatalogRepository {
    type Error = PersistenceError;
    type Id = String;
    type Product = Product;

    async fn find_by_id(&self, id: &Self::Id) -> Result<Option<Self::Product>, Self::Error> {
        let uuid = parse_uuid(id)?;
        let row = product::Entity::find_by_id(uuid)
            .one(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        Ok(row.map(product_from_model))
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Self::Product>, Self::Error> {
        let row = product::Entity::find()
            .filter(product::Column::Slug.eq(slug))
            .one(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        Ok(row.map(product_from_model))
    }

    async fn list(&self, page: PageRequest) -> Result<Vec<Self::Product>, Self::Error> {
        let rows = product::Entity::find()
            .filter(product::Column::Enabled.eq(true))
            .order_by_asc(product::Column::Slug)
            .limit(u64::from(page.limit))
            .offset(u64::from(page.offset))
            .all(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        Ok(rows.into_iter().map(product_from_model).collect())
    }
}

impl CategoryRepository for SeaOrmCatalogRepository {
    type Error = PersistenceError;
    type Id = String;
    type Category = Category;

    async fn find_by_id(&self, id: &Self::Id) -> Result<Option<Self::Category>, Self::Error> {
        let uuid = parse_uuid(id)?;
        let row = category::Entity::find_by_id(uuid)
            .one(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        Ok(row.map(category_from_model))
    }

    async fn find_by_slug(
        &self,
        slug: &str,
        parent_id: Option<&Self::Id>,
    ) -> Result<Option<Self::Category>, Self::Error> {
        let mut query = category::Entity::find().filter(category::Column::Slug.eq(slug));
        query = match parent_id {
            Some(id) => query.filter(category::Column::ParentId.eq(parse_uuid(id)?)),
            None => query.filter(category::Column::ParentId.is_null()),
        };
        let row = query
            .one(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        Ok(row.map(category_from_model))
    }

    async fn list_children(
        &self,
        parent_id: Option<&Self::Id>,
        page: PageRequest,
    ) -> Result<Vec<Self::Category>, Self::Error> {
        let mut query = category::Entity::find();
        query = match parent_id {
            Some(id) => query.filter(category::Column::ParentId.eq(parse_uuid(id)?)),
            None => query.filter(category::Column::ParentId.is_null()),
        };
        let rows = query
            .order_by_asc(category::Column::Slug)
            .limit(u64::from(page.limit))
            .offset(u64::from(page.offset))
            .all(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        Ok(rows.into_iter().map(category_from_model).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustashop_domain::{CategoryRepository, ProductRepository};

    fn assert_product_repo<T: ProductRepository>() {}
    fn assert_category_repo<T: CategoryRepository>() {}

    #[test]
    fn seaorm_catalog_implements_repository_traits() {
        assert_product_repo::<SeaOrmCatalogRepository>();
        assert_category_repo::<SeaOrmCatalogRepository>();
    }
}
