//! Admin order reads and state updates.

use rustashop_domain::{Order, OrderState};
use serenade_contracts::{PageRequest, PersistenceError};
use sqlx::postgres::PgPool;

use crate::SqlxCatalogRepository;
use crate::checkout::{OrderRow, load_order_lines_pool, order_from_rows};

fn internal(error: &sqlx::Error) -> PersistenceError {
    PersistenceError::Internal {
        message: error.to_string(),
    }
}

impl SqlxCatalogRepository {
    /// Lists orders newest-first with pagination.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::Internal`] on query failure.
    pub async fn list_orders(&self, page: PageRequest) -> Result<Vec<Order>, PersistenceError> {
        let rows = sqlx::query_as::<_, OrderRow>(
            r#"SELECT id::text AS id, number, customer_id::text AS customer_id,
                      cart_id::text AS cart_id, state, currency,
                      items_total_minor, total_minor, idempotency_key
               FROM "order"
               ORDER BY created_at DESC, id DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(i64::from(page.limit))
        .bind(i64::from(page.offset))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        let mut orders = Vec::with_capacity(rows.len());
        for row in rows {
            let lines = load_order_lines_pool(&self.pool, &row.id).await?;
            orders.push(order_from_rows(row, lines)?);
        }
        Ok(orders)
    }

    /// Loads one order by id.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::NotFound`] when missing, or internal errors.
    pub async fn get_order(&self, order_id: &str) -> Result<Order, PersistenceError> {
        load_order_pool(&self.pool, order_id).await
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
        let result =
            sqlx::query(r#"UPDATE "order" SET state = $2, updated_at = NOW() WHERE id = $1::uuid"#)
                .bind(order_id)
                .bind(state.as_str())
                .execute(&self.pool)
                .await
                .map_err(|error| internal(&error))?;
        if result.rows_affected() == 0 {
            return Err(PersistenceError::NotFound {
                entity: "order",
                id: order_id.to_owned(),
            });
        }
        load_order_pool(&self.pool, order_id).await
    }
}

async fn load_order_pool(pool: &PgPool, order_id: &str) -> Result<Order, PersistenceError> {
    let row = sqlx::query_as::<_, OrderRow>(
        r#"SELECT id::text AS id, number, customer_id::text AS customer_id,
                  cart_id::text AS cart_id, state, currency,
                  items_total_minor, total_minor, idempotency_key
           FROM "order" WHERE id = $1::uuid"#,
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| internal(&error))?;
    let Some(row) = row else {
        return Err(PersistenceError::NotFound {
            entity: "order",
            id: order_id.to_owned(),
        });
    };
    let lines = load_order_lines_pool(pool, order_id).await?;
    order_from_rows(row, lines)
}
