//! `SQLx` cart repository and line mutations.

use rustashop_domain::{
    Cart, CartLine, CartRepository, CartStatus, Currency, Money, ProductVariant,
};
use serenade_contracts::PersistenceError;
use sqlx::postgres::PgPool;
use sqlx::{FromRow, Postgres, Transaction};

use crate::SqlxCatalogRepository;

#[derive(Debug, FromRow)]
struct CartRow {
    id: String,
    customer_id: Option<String>,
    token: String,
    currency: String,
    status: String,
}

#[derive(Debug, FromRow)]
struct CartLineRow {
    id: String,
    cart_id: String,
    variant_id: String,
    quantity: i32,
    unit_price_minor: i64,
    currency: String,
    product_name: String,
    variant_sku: String,
}

#[derive(Debug, FromRow)]
struct VariantJoinRow {
    id: String,
    product_id: String,
    sku: String,
    name: Option<String>,
    price_minor: i64,
    currency: String,
    stock_quantity: i32,
    product_name: String,
}

fn internal(error: &sqlx::Error) -> PersistenceError {
    PersistenceError::Internal {
        message: error.to_string(),
    }
}

fn money(amount_minor: i64, currency: &str) -> Result<Money, PersistenceError> {
    let currency = Currency::new(currency).map_err(|error| PersistenceError::InvalidInput {
        message: error.to_string(),
    })?;
    Ok(Money::new(amount_minor, currency))
}

fn cart_from_rows(row: CartRow, lines: Vec<CartLineRow>) -> Result<Cart, PersistenceError> {
    let currency =
        Currency::new(&row.currency).map_err(|error| PersistenceError::InvalidInput {
            message: error.to_string(),
        })?;
    let mut cart_lines = Vec::with_capacity(lines.len());
    for line in lines {
        cart_lines.push(CartLine {
            id: line.id,
            cart_id: line.cart_id,
            variant_id: line.variant_id,
            quantity: line.quantity,
            unit_price: money(line.unit_price_minor, &line.currency)?,
            product_name: line.product_name,
            variant_sku: line.variant_sku,
        });
    }
    Ok(Cart {
        id: row.id,
        customer_id: row.customer_id,
        token: row.token,
        currency,
        status: CartStatus::parse(&row.status).map_err(|error| PersistenceError::InvalidInput {
            message: error.to_string(),
        })?,
        lines: cart_lines,
    })
}

impl SqlxCatalogRepository {
    /// Creates an empty cart in `currency`.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError`] when the insert fails or the currency is invalid.
    pub async fn create_cart(&self, currency: &Currency) -> Result<Cart, PersistenceError> {
        let row = sqlx::query_as::<_, CartRow>(
            "INSERT INTO cart (token, currency)
             VALUES (gen_random_uuid()::text, $1)
             RETURNING id::text AS id, customer_id::text AS customer_id, token, currency, status",
        )
        .bind(currency.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        cart_from_rows(row, Vec::new())
    }

    /// Loads a cart and its lines by id.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError`] on query failure.
    pub async fn find_cart_by_id(&self, id: &str) -> Result<Option<Cart>, PersistenceError> {
        let row = sqlx::query_as::<_, CartRow>(
            "SELECT id::text AS id, customer_id::text AS customer_id, token, currency, status
             FROM cart WHERE id = $1::uuid",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let lines = load_lines(&self.pool, &row.id).await?;
        Ok(Some(cart_from_rows(row, lines)?))
    }

    /// Loads a variant with its parent product name for cart snapshots.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError`] on query failure.
    pub async fn find_variant_for_cart(
        &self,
        variant_id: &str,
    ) -> Result<Option<(ProductVariant, String)>, PersistenceError> {
        let row = sqlx::query_as::<_, VariantJoinRow>(
            "SELECT pv.id::text AS id, pv.product_id::text AS product_id, pv.sku, pv.name,
                    pv.price_minor, pv.currency, pv.stock_quantity, p.name AS product_name
             FROM product_variant pv
             INNER JOIN product p ON p.id = pv.product_id
             WHERE pv.id = $1::uuid",
        )
        .bind(variant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some((
            ProductVariant {
                id: row.id,
                product_id: row.product_id,
                sku: row.sku,
                name: row.name,
                price: money(row.price_minor, &row.currency)?,
                stock_quantity: row.stock_quantity,
            },
            row.product_name,
        )))
    }

    /// Persists cart header and replaces all lines inside a transaction.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError`] when the transaction fails.
    pub async fn save_cart(&self, cart: &Cart) -> Result<(), PersistenceError> {
        let mut tx = self.pool.begin().await.map_err(|error| internal(&error))?;
        save_cart_tx(&mut tx, cart).await?;
        tx.commit().await.map_err(|error| internal(&error))?;
        Ok(())
    }
}

async fn load_lines(pool: &PgPool, cart_id: &str) -> Result<Vec<CartLineRow>, PersistenceError> {
    sqlx::query_as::<_, CartLineRow>(
        "SELECT id::text AS id, cart_id::text AS cart_id, variant_id::text AS variant_id,
                quantity, unit_price_minor, currency, product_name, variant_sku
         FROM cart_line WHERE cart_id = $1::uuid ORDER BY created_at, id",
    )
    .bind(cart_id)
    .fetch_all(pool)
    .await
    .map_err(|error| internal(&error))
}

async fn save_cart_tx(
    tx: &mut Transaction<'_, Postgres>,
    cart: &Cart,
) -> Result<(), PersistenceError> {
    let updated = sqlx::query(
        "UPDATE cart SET customer_id = $2::uuid, currency = $3, status = $4, updated_at = NOW()
         WHERE id = $1::uuid",
    )
    .bind(&cart.id)
    .bind(cart.customer_id.as_deref())
    .bind(cart.currency.as_str())
    .bind(cart.status.as_str())
    .execute(&mut **tx)
    .await
    .map_err(|error| internal(&error))?;
    if updated.rows_affected() == 0 {
        return Err(PersistenceError::NotFound {
            entity: "cart",
            id: cart.id.clone(),
        });
    }
    sqlx::query("DELETE FROM cart_line WHERE cart_id = $1::uuid")
        .bind(&cart.id)
        .execute(&mut **tx)
        .await
        .map_err(|error| internal(&error))?;
    for line in &cart.lines {
        let line_id = if line.id.is_empty() {
            None
        } else {
            Some(line.id.as_str())
        };
        sqlx::query(
            "INSERT INTO cart_line (
                id, cart_id, variant_id, quantity, unit_price_minor, currency,
                product_name, variant_sku
             ) VALUES (
                COALESCE($1::uuid, gen_random_uuid()), $2::uuid, $3::uuid, $4, $5, $6, $7, $8
             )",
        )
        .bind(line_id)
        .bind(&cart.id)
        .bind(&line.variant_id)
        .bind(line.quantity)
        .bind(line.unit_price.amount_minor)
        .bind(line.unit_price.currency.as_str())
        .bind(&line.product_name)
        .bind(&line.variant_sku)
        .execute(&mut **tx)
        .await
        .map_err(|error| internal(&error))?;
    }
    Ok(())
}

impl CartRepository for SqlxCatalogRepository {
    type Error = PersistenceError;
    type Id = String;
    type Cart = Cart;

    async fn find_by_token(&self, token: &str) -> Result<Option<Self::Cart>, Self::Error> {
        let row = sqlx::query_as::<_, CartRow>(
            "SELECT id::text AS id, customer_id::text AS customer_id, token, currency, status
             FROM cart WHERE token = $1",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| internal(&error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let lines = load_lines(&self.pool, &row.id).await?;
        Ok(Some(cart_from_rows(row, lines)?))
    }

    async fn save(&self, cart: &Self::Cart) -> Result<(), Self::Error> {
        self.save_cart(cart).await
    }

    async fn delete(&self, id: &Self::Id) -> Result<(), Self::Error> {
        sqlx::query("DELETE FROM cart WHERE id = $1::uuid")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|error| internal(&error))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustashop_domain::CartRepository;

    fn assert_cart_repo<T: CartRepository>() {}

    #[test]
    fn sqlx_catalog_implements_cart_repository() {
        assert_cart_repo::<SqlxCatalogRepository>();
    }

    #[test]
    fn money_and_cart_from_rows_helpers() {
        assert!(money(1, "x").is_err());
        assert_eq!(money(5, "EUR").unwrap().amount_minor, 5);
        let err = internal(&sqlx::Error::Protocol("x".into()));
        assert!(matches!(err, PersistenceError::Internal { .. }));
        let cart = cart_from_rows(
            CartRow {
                id: "c1".into(),
                customer_id: Some("u1".into()),
                token: "tok".into(),
                currency: "EUR".into(),
                status: "open".into(),
            },
            vec![],
        )
        .unwrap();
        assert_eq!(cart.id, "c1");
        assert_eq!(cart.lines.len(), 0);
    }

    #[test]
    fn cart_from_rows_rejects_bad_currency_and_status() {
        assert!(matches!(
            cart_from_rows(
                CartRow {
                    id: "c1".into(),
                    customer_id: None,
                    token: "tok".into(),
                    currency: "ZZ".into(),
                    status: "open".into(),
                },
                vec![],
            ),
            Err(PersistenceError::InvalidInput { .. })
        ));
        assert!(matches!(
            cart_from_rows(
                CartRow {
                    id: "c1".into(),
                    customer_id: None,
                    token: "tok".into(),
                    currency: "EUR".into(),
                    status: "nope".into(),
                },
                vec![],
            ),
            Err(PersistenceError::InvalidInput { .. })
        ));
        assert!(matches!(
            cart_from_rows(
                CartRow {
                    id: "c1".into(),
                    customer_id: None,
                    token: "tok".into(),
                    currency: "EUR".into(),
                    status: "open".into(),
                },
                vec![CartLineRow {
                    id: "l1".into(),
                    cart_id: "c1".into(),
                    variant_id: "v1".into(),
                    quantity: 1,
                    unit_price_minor: 100,
                    currency: "ZZ".into(),
                    product_name: "Mug".into(),
                    variant_sku: "MUG".into(),
                }],
            ),
            Err(PersistenceError::InvalidInput { .. })
        ));
    }
}
