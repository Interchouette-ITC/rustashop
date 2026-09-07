//! Checkout: place an order from a cart inside one transaction.

use rustashop_domain::{Cart, CartLine, CartStatus, Currency, Money, Order, OrderLine};
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
pub(crate) struct OrderRow {
    pub(crate) id: String,
    pub(crate) number: String,
    pub(crate) customer_id: Option<String>,
    pub(crate) cart_id: Option<String>,
    pub(crate) state: String,
    pub(crate) currency: String,
    pub(crate) items_total_minor: i64,
    pub(crate) total_minor: i64,
    pub(crate) idempotency_key: Option<String>,
}

#[derive(Debug, FromRow)]
pub(crate) struct OrderLineRow {
    pub(crate) id: String,
    pub(crate) order_id: String,
    pub(crate) variant_id: Option<String>,
    pub(crate) quantity: i32,
    pub(crate) unit_price_minor: i64,
    pub(crate) line_total_minor: i64,
    pub(crate) currency: String,
    pub(crate) product_name: String,
    pub(crate) variant_sku: String,
}

fn internal(error: &sqlx::Error) -> PersistenceError {
    PersistenceError::Internal {
        message: error.to_string(),
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .as_deref()
        == Some("23505")
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

pub(crate) fn order_from_rows(
    row: OrderRow,
    lines: Vec<OrderLineRow>,
) -> Result<Order, PersistenceError> {
    let code = row.currency.clone();
    let currency = Currency::new(&code).map_err(|error| PersistenceError::InvalidInput {
        message: error.to_string(),
    })?;
    let mut order_lines = Vec::with_capacity(lines.len());
    for line in lines {
        order_lines.push(OrderLine {
            id: line.id,
            order_id: line.order_id,
            variant_id: line.variant_id,
            quantity: line.quantity,
            unit_price: money(line.unit_price_minor, &line.currency)?,
            line_total: money(line.line_total_minor, &line.currency)?,
            product_name: line.product_name,
            variant_sku: line.variant_sku,
        });
    }
    Ok(Order {
        id: row.id,
        number: row.number,
        cart_id: row.cart_id,
        customer_id: row.customer_id,
        state: row.state,
        payment_status: rustashop_domain::PAYMENT_STATUS_PENDING.to_owned(),
        items_total: money(row.items_total_minor, &code)?,
        total: money(row.total_minor, &code)?,
        currency,
        idempotency_key: row.idempotency_key,
        lines: order_lines,
    })
}

impl SqlxCatalogRepository {
    /// Places an order from `cart_id`. Replays when `idempotency_key` already exists.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::NotFound`] when the cart is missing,
    /// [`PersistenceError::InvalidInput`] for an empty cart,
    /// [`PersistenceError::Conflict`] when the cart is already checked out
    /// without a matching key, or internal errors on query failure.
    pub async fn checkout_cart(
        &self,
        cart_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<Order, PersistenceError> {
        if let Some(key) = idempotency_key
            && let Some(existing) = find_order_by_key(&self.pool, key).await?
        {
            return Ok(existing);
        }
        let mut tx = self.pool.begin().await.map_err(|error| internal(&error))?;
        match checkout_tx(&mut tx, cart_id, idempotency_key).await {
            Ok(order) => {
                tx.commit().await.map_err(|error| internal(&error))?;
                Ok(order)
            }
            Err(error) => {
                tx.rollback().await.ok();
                if let Some(key) = idempotency_key
                    && is_conflict_unique(&error)
                {
                    return find_order_by_key(&self.pool, key).await?.ok_or(error);
                }
                Err(error)
            }
        }
    }
}

fn is_conflict_unique(error: &PersistenceError) -> bool {
    matches!(
        error,
        PersistenceError::Conflict {
            constraint: "idempotency_key"
        }
    )
}

async fn checkout_tx(
    tx: &mut Transaction<'_, Postgres>,
    cart_id: &str,
    idempotency_key: Option<&str>,
) -> Result<Order, PersistenceError> {
    let row = sqlx::query_as::<_, CartRow>(
        "SELECT id::text AS id, customer_id::text AS customer_id, token, currency, status
         FROM cart WHERE id = $1::uuid FOR UPDATE",
    )
    .bind(cart_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| internal(&error))?;
    let Some(row) = row else {
        return Err(PersistenceError::NotFound {
            entity: "cart",
            id: cart_id.to_owned(),
        });
    };
    let lines = sqlx::query_as::<_, CartLineRow>(
        "SELECT id::text AS id, cart_id::text AS cart_id, variant_id::text AS variant_id,
                quantity, unit_price_minor, currency, product_name, variant_sku
         FROM cart_line WHERE cart_id = $1::uuid ORDER BY created_at, id",
    )
    .bind(cart_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| internal(&error))?;
    let cart = cart_from_rows(row, lines)?;
    if cart.status == CartStatus::CheckedOut {
        if let Some(key) = idempotency_key
            && let Some(existing) = find_order_by_key_tx(tx, key).await?
        {
            return Ok(existing);
        }
        return Err(PersistenceError::Conflict {
            constraint: "cart_status",
        });
    }
    cart.ensure_checkoutable()
        .map_err(|error| PersistenceError::InvalidInput {
            message: error.to_string(),
        })?;
    let items_total = cart
        .items_total()
        .map_err(|error| PersistenceError::InvalidInput {
            message: error.to_string(),
        })?;
    let inserted = sqlx::query_as::<_, OrderRow>(
        r#"INSERT INTO "order" (
                number, customer_id, cart_id, state, currency,
                items_total_minor, total_minor, idempotency_key, placed_at
            ) VALUES (
                'RS-' || substr(replace(gen_random_uuid()::text, '-', ''), 1, 12),
                $1::uuid, $2::uuid, 'placed', $3, $4, $4, $5, NOW()
            )
            RETURNING id::text AS id, number, customer_id::text AS customer_id,
                      cart_id::text AS cart_id, state, currency,
                      items_total_minor, total_minor, idempotency_key"#,
    )
    .bind(cart.customer_id.as_deref())
    .bind(&cart.id)
    .bind(cart.currency.as_str())
    .bind(items_total.amount_minor)
    .bind(idempotency_key)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| {
        if is_unique_violation(&error) {
            PersistenceError::Conflict {
                constraint: "idempotency_key",
            }
        } else {
            internal(&error)
        }
    })?;
    insert_order_lines(tx, &inserted.id, &cart).await?;
    sqlx::query("UPDATE cart SET status = 'checked_out', updated_at = NOW() WHERE id = $1::uuid")
        .bind(&cart.id)
        .execute(&mut **tx)
        .await
        .map_err(|error| internal(&error))?;
    load_order_tx(tx, &inserted.id).await
}

async fn insert_order_lines(
    tx: &mut Transaction<'_, Postgres>,
    order_id: &str,
    cart: &Cart,
) -> Result<(), PersistenceError> {
    for line in &cart.lines {
        let line_total = line
            .line_total()
            .expect("line totals already validated by items_total");
        sqlx::query(
            "INSERT INTO order_line (
                order_id, variant_id, quantity, unit_price_minor, line_total_minor,
                currency, product_name, variant_sku
             ) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8)",
        )
        .bind(order_id)
        .bind(&line.variant_id)
        .bind(line.quantity)
        .bind(line.unit_price.amount_minor)
        .bind(line_total.amount_minor)
        .bind(line.unit_price.currency.as_str())
        .bind(&line.product_name)
        .bind(&line.variant_sku)
        .execute(&mut **tx)
        .await
        .map_err(|error| internal(&error))?;
    }
    Ok(())
}

async fn find_order_by_key(pool: &PgPool, key: &str) -> Result<Option<Order>, PersistenceError> {
    let row = sqlx::query_as::<_, OrderRow>(
        r#"SELECT id::text AS id, number, customer_id::text AS customer_id,
                  cart_id::text AS cart_id, state, currency,
                  items_total_minor, total_minor, idempotency_key
           FROM "order" WHERE idempotency_key = $1"#,
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(|error| internal(&error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let lines = load_order_lines_pool(pool, &row.id).await?;
    Ok(Some(order_from_rows(row, lines)?))
}

pub(crate) async fn find_order_by_key_tx(
    tx: &mut Transaction<'_, Postgres>,
    key: &str,
) -> Result<Option<Order>, PersistenceError> {
    let row = sqlx::query_as::<_, OrderRow>(
        r#"SELECT id::text AS id, number, customer_id::text AS customer_id,
                  cart_id::text AS cart_id, state, currency,
                  items_total_minor, total_minor, idempotency_key
           FROM "order" WHERE idempotency_key = $1"#,
    )
    .bind(key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| internal(&error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let lines = load_order_lines_tx(tx, &row.id).await?;
    Ok(Some(order_from_rows(row, lines)?))
}

pub(crate) async fn load_order_tx(
    tx: &mut Transaction<'_, Postgres>,
    order_id: &str,
) -> Result<Order, PersistenceError> {
    let row = sqlx::query_as::<_, OrderRow>(
        r#"SELECT id::text AS id, number, customer_id::text AS customer_id,
                  cart_id::text AS cart_id, state, currency,
                  items_total_minor, total_minor, idempotency_key
           FROM "order" WHERE id = $1::uuid"#,
    )
    .bind(order_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| internal(&error))?;
    let lines = load_order_lines_tx(tx, order_id).await?;
    order_from_rows(row, lines)
}

pub(crate) async fn load_order_lines_tx(
    tx: &mut Transaction<'_, Postgres>,
    order_id: &str,
) -> Result<Vec<OrderLineRow>, PersistenceError> {
    sqlx::query_as::<_, OrderLineRow>(
        "SELECT id::text AS id, order_id::text AS order_id, variant_id::text AS variant_id,
                quantity, unit_price_minor, line_total_minor, currency, product_name, variant_sku
         FROM order_line WHERE order_id = $1::uuid ORDER BY created_at, id",
    )
    .bind(order_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| internal(&error))
}

pub(crate) async fn load_order_lines_pool(
    pool: &PgPool,
    order_id: &str,
) -> Result<Vec<OrderLineRow>, PersistenceError> {
    sqlx::query_as::<_, OrderLineRow>(
        "SELECT id::text AS id, order_id::text AS order_id, variant_id::text AS variant_id,
                quantity, unit_price_minor, line_total_minor, currency, product_name, variant_sku
         FROM order_line WHERE order_id = $1::uuid ORDER BY created_at, id",
    )
    .bind(order_id)
    .fetch_all(pool)
    .await
    .map_err(|error| internal(&error))
}

#[cfg(test)]
mod helper_tests {
    use super::*;
    use rustashop_domain::CartStatus;

    #[test]
    fn money_rejects_invalid_currency() {
        assert!(money(10, "no").is_err());
        assert_eq!(money(10, "EUR").unwrap().amount_minor, 10);
    }

    #[test]
    fn cart_from_rows_maps_open_cart() {
        let cart = cart_from_rows(
            CartRow {
                id: "c1".into(),
                customer_id: None,
                token: "t".into(),
                currency: "EUR".into(),
                status: "open".into(),
            },
            vec![CartLineRow {
                id: "l1".into(),
                cart_id: "c1".into(),
                variant_id: "v1".into(),
                quantity: 2,
                unit_price_minor: 100,
                currency: "EUR".into(),
                product_name: "Mug".into(),
                variant_sku: "MUG".into(),
            }],
        )
        .unwrap();
        assert_eq!(cart.status, CartStatus::Open);
        assert_eq!(cart.lines.len(), 1);
        assert_eq!(cart.lines[0].quantity, 2);
    }

    #[test]
    fn cart_from_rows_rejects_bad_status() {
        assert!(
            cart_from_rows(
                CartRow {
                    id: "c1".into(),
                    customer_id: None,
                    token: "t".into(),
                    currency: "EUR".into(),
                    status: "nope".into(),
                },
                vec![],
            )
            .is_err()
        );
    }

    #[test]
    fn cart_from_rows_rejects_bad_currency() {
        assert!(matches!(
            cart_from_rows(
                CartRow {
                    id: "c1".into(),
                    customer_id: None,
                    token: "t".into(),
                    currency: "ZZ".into(),
                    status: "open".into(),
                },
                vec![],
            ),
            Err(PersistenceError::InvalidInput { .. })
        ));
    }

    #[test]
    fn order_from_rows_rejects_bad_currency() {
        assert!(matches!(
            order_from_rows(
                OrderRow {
                    id: "o1".into(),
                    number: "RS-1".into(),
                    customer_id: None,
                    cart_id: Some("c1".into()),
                    state: "placed".into(),
                    currency: "ZZ".into(),
                    items_total_minor: 200,
                    total_minor: 200,
                    idempotency_key: None,
                },
                vec![],
            ),
            Err(PersistenceError::InvalidInput { .. })
        ));
        assert!(matches!(
            order_from_rows(
                OrderRow {
                    id: "o1".into(),
                    number: "RS-1".into(),
                    customer_id: None,
                    cart_id: Some("c1".into()),
                    state: "placed".into(),
                    currency: "EUR".into(),
                    items_total_minor: 200,
                    total_minor: 200,
                    idempotency_key: None,
                },
                vec![OrderLineRow {
                    id: "ol1".into(),
                    order_id: "o1".into(),
                    variant_id: Some("v1".into()),
                    quantity: 2,
                    unit_price_minor: 100,
                    line_total_minor: 200,
                    currency: "ZZ".into(),
                    product_name: "Mug".into(),
                    variant_sku: "MUG".into(),
                }],
            ),
            Err(PersistenceError::InvalidInput { .. })
        ));
    }

    #[test]
    fn order_from_rows_builds_pending_order() {
        let order = order_from_rows(
            OrderRow {
                id: "o1".into(),
                number: "RS-1".into(),
                customer_id: None,
                cart_id: Some("c1".into()),
                state: "placed".into(),
                currency: "EUR".into(),
                items_total_minor: 200,
                total_minor: 200,
                idempotency_key: Some("k".into()),
            },
            vec![OrderLineRow {
                id: "ol1".into(),
                order_id: "o1".into(),
                variant_id: Some("v1".into()),
                quantity: 2,
                unit_price_minor: 100,
                line_total_minor: 200,
                currency: "EUR".into(),
                product_name: "Mug".into(),
                variant_sku: "MUG".into(),
            }],
        )
        .unwrap();
        assert_eq!(order.total.amount_minor, 200);
        assert_eq!(order.lines.len(), 1);
        assert_eq!(
            order.payment_status,
            rustashop_domain::PAYMENT_STATUS_PENDING
        );
    }

    #[test]
    fn conflict_unique_matcher() {
        assert!(is_conflict_unique(&PersistenceError::Conflict {
            constraint: "idempotency_key"
        }));
        assert!(!is_conflict_unique(&PersistenceError::NotFound {
            entity: "cart",
            id: "x".into(),
        }));
    }

    #[test]
    fn internal_maps_message() {
        let err = internal(&sqlx::Error::Protocol("boom".into()));
        assert!(matches!(err, PersistenceError::Internal { .. }));
    }

    #[tokio::test]
    async fn load_helpers_map_internal_when_order_tables_missing() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect");
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(874_531_i64)
            .execute(&pool)
            .await
            .expect("lock");
        sqlx::query("DROP SCHEMA public CASCADE")
            .execute(&pool)
            .await
            .expect("drop");
        sqlx::query("CREATE SCHEMA public")
            .execute(&pool)
            .await
            .expect("create");
        crate::migrate(&pool).await.expect("migrate");

        sqlx::query("DROP TABLE order_line CASCADE")
            .execute(&pool)
            .await
            .expect("drop order_line");
        let err = load_order_lines_pool(&pool, "00000000-0000-0000-0000-000000000001")
            .await
            .expect_err("lines pool");
        assert!(matches!(err, PersistenceError::Internal { .. }));

        let mut tx = pool.begin().await.expect("begin");
        let err = load_order_lines_tx(&mut tx, "00000000-0000-0000-0000-000000000001")
            .await
            .expect_err("lines tx");
        assert!(matches!(err, PersistenceError::Internal { .. }));
        let _ = tx.rollback().await;

        sqlx::query(r#"DROP TABLE "order" CASCADE"#)
            .execute(&pool)
            .await
            .expect("drop order");
        let mut tx = pool.begin().await.expect("begin");
        let err = load_order_tx(&mut tx, "00000000-0000-0000-0000-000000000001")
            .await
            .expect_err("load order");
        assert!(matches!(err, PersistenceError::Internal { .. }));
        let err = find_order_by_key_tx(&mut tx, "missing-key")
            .await
            .expect_err("find key tx");
        assert!(matches!(err, PersistenceError::Internal { .. }));
        let _ = tx.rollback().await;

        sqlx::query("DROP SCHEMA public CASCADE")
            .execute(&pool)
            .await
            .expect("cleanup drop");
        sqlx::query("CREATE SCHEMA public")
            .execute(&pool)
            .await
            .expect("cleanup create");
        crate::migrate(&pool).await.expect("migrate again");
        crate::seed_catalog(&pool).await.expect("seed");
        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(874_531_i64)
            .execute(&pool)
            .await
            .expect("unlock");
    }
}
