//! Admin order reads and state updates.

use rustashop_domain::{Order, OrderState};
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, QuerySelect, Set};
use serenade_contracts::{PageRequest, PersistenceError};
use uuid::Uuid;

use crate::SeaOrmCatalogRepository;
use crate::checkout::{load_order, load_order_lines, order_from_models};
use crate::entities::commerce_order;

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

impl SeaOrmCatalogRepository {
    /// Lists orders newest-first with pagination.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::Internal`] on query failure.
    pub async fn list_orders(&self, page: PageRequest) -> Result<Vec<Order>, PersistenceError> {
        let models = commerce_order::Entity::find()
            .order_by_desc(commerce_order::Column::CreatedAt)
            .order_by_desc(commerce_order::Column::Id)
            .limit(u64::from(page.limit))
            .offset(u64::from(page.offset))
            .all(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        let mut orders = Vec::with_capacity(models.len());
        for model in models {
            let lines = load_order_lines(&self.db, model.id).await?;
            orders.push(order_from_models(model, lines)?);
        }
        Ok(orders)
    }

    /// Loads one order by id.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::NotFound`] when missing, or internal errors.
    pub async fn get_order(&self, order_id: &str) -> Result<Order, PersistenceError> {
        let id = parse_uuid(order_id)?;
        load_order(&self.db, id).await
    }

    /// Updates `"order".state` and returns the full order.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::NotFound`] when missing, or internal errors.
    pub async fn update_order_state(
        &self,
        order_id: &str,
        state: OrderState,
    ) -> Result<Order, PersistenceError> {
        let id = parse_uuid(order_id)?;
        let model = commerce_order::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|error| internal(&error))?
            .ok_or_else(|| PersistenceError::NotFound {
                entity: "order",
                id: order_id.to_owned(),
            })?;
        let mut active: commerce_order::ActiveModel = model.into();
        active.state = Set(state.as_str().to_owned());
        active.updated_at = Set(chrono::Utc::now().into());
        active
            .update(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        load_order(&self.db, id).await
    }
}
