//! Checkout: place an order from a cart inside one transaction.

use rustashop_domain::{
    Cart, CartStatus, Currency, Money, Order, OrderLine, PAYMENT_STATUS_PENDING,
};
use sea_orm::entity::prelude::*;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait,
};
use serenade_contracts::PersistenceError;

use crate::SeaOrmCatalogRepository;
use crate::cart::cart_from_models;
use crate::entities::{cart, cart_line, commerce_order, order_line};
use uuid::Uuid;

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

fn is_unique_violation(error: &DbErr) -> bool {
    error.to_string().contains("duplicate key")
}

fn money(amount_minor: i64, currency: &str) -> Result<Money, PersistenceError> {
    let currency = Currency::new(currency).map_err(|error| PersistenceError::InvalidInput {
        message: error.to_string(),
    })?;
    Ok(Money::new(amount_minor, currency))
}

pub(crate) fn order_from_models(
    model: commerce_order::Model,
    lines: Vec<order_line::Model>,
) -> Result<Order, PersistenceError> {
    let code = model.currency.clone();
    let currency = Currency::new(&code).map_err(|error| PersistenceError::InvalidInput {
        message: error.to_string(),
    })?;
    let mut order_lines = Vec::with_capacity(lines.len());
    for line in lines {
        order_lines.push(OrderLine {
            id: line.id.to_string(),
            order_id: line.order_id.to_string(),
            variant_id: line.variant_id.map(|id| id.to_string()),
            quantity: line.quantity,
            unit_price: money(line.unit_price_minor, &line.currency)?,
            line_total: money(line.line_total_minor, &line.currency)?,
            product_name: line.product_name,
            variant_sku: line.variant_sku,
        });
    }
    Ok(Order {
        id: model.id.to_string(),
        number: model.number,
        cart_id: model.cart_id.map(|id| id.to_string()),
        customer_id: model.customer_id.map(|id| id.to_string()),
        state: model.state,
        payment_status: PAYMENT_STATUS_PENDING.to_owned(),
        items_total: money(model.items_total_minor, &code)?,
        total: money(model.total_minor, &code)?,
        currency,
        idempotency_key: model.idempotency_key,
        lines: order_lines,
    })
}

impl SeaOrmCatalogRepository {
    /// Places an order from `cart_id`. Replays when `idempotency_key` already exists.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::NotFound`], [`PersistenceError::InvalidInput`],
    /// [`PersistenceError::Conflict`], or an internal adapter error.
    pub async fn checkout_cart(
        &self,
        cart_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<Order, PersistenceError> {
        if let Some(key) = idempotency_key
            && let Some(existing) = find_order_by_key(&self.db, key).await?
        {
            return Ok(existing);
        }
        let txn = self.db.begin().await.map_err(|error| internal(&error))?;
        match checkout_in_txn(&txn, cart_id, idempotency_key).await {
            Ok(order) => {
                txn.commit().await.map_err(|error| internal(&error))?;
                Ok(order)
            }
            Err(error) => {
                txn.rollback().await.ok();
                if let Some(key) = idempotency_key
                    && matches!(
                        error,
                        PersistenceError::Conflict {
                            constraint: "idempotency_key"
                        }
                    )
                {
                    return find_order_by_key(&self.db, key).await?.ok_or(error);
                }
                Err(error)
            }
        }
    }
}

async fn checkout_in_txn<C: ConnectionTrait>(
    db: &C,
    cart_id: &str,
    idempotency_key: Option<&str>,
) -> Result<Order, PersistenceError> {
    let uuid = parse_uuid(cart_id)?;
    let model = cart::Entity::find_by_id(uuid)
        .lock_exclusive()
        .one(db)
        .await
        .map_err(|error| internal(&error))?
        .ok_or_else(|| PersistenceError::NotFound {
            entity: "cart",
            id: cart_id.to_owned(),
        })?;
    let lines = cart_line::Entity::find()
        .filter(cart_line::Column::CartId.eq(uuid))
        .order_by_asc(cart_line::Column::CreatedAt)
        .order_by_asc(cart_line::Column::Id)
        .all(db)
        .await
        .map_err(|error| internal(&error))?;
    let cart = cart_from_models(model, lines)?;
    if cart.status == CartStatus::CheckedOut {
        if let Some(key) = idempotency_key
            && let Some(existing) = find_order_by_key(db, key).await?
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
    let now = chrono::Utc::now().into();
    let order_id = Uuid::new_v4();
    let number = format!("RS-{}", &order_id.simple().to_string()[..12]);
    let inserted = commerce_order::ActiveModel {
        id: Set(order_id),
        number: Set(number),
        customer_id: Set(match &cart.customer_id {
            Some(id) => Some(parse_uuid(id)?),
            None => None,
        }),
        cart_id: Set(Some(uuid)),
        state: Set("placed".to_owned()),
        currency: Set(cart.currency.as_str().to_owned()),
        items_total_minor: Set(items_total.amount_minor),
        total_minor: Set(items_total.amount_minor),
        idempotency_key: Set(idempotency_key.map(ToOwned::to_owned)),
        created_at: Set(now),
        updated_at: Set(now),
        placed_at: Set(Some(now)),
    }
    .insert(db)
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
    insert_order_lines(db, inserted.id, &cart, now).await?;
    mark_cart_checked_out(db, uuid, &cart.id, now).await?;
    load_order(db, inserted.id).await
}

async fn insert_order_lines<C: ConnectionTrait>(
    db: &C,
    order_id: Uuid,
    cart: &Cart,
    now: DateTimeWithTimeZone,
) -> Result<(), PersistenceError> {
    for line in &cart.lines {
        let line_total = line
            .line_total()
            .expect("line totals already validated by items_total");
        order_line::ActiveModel {
            id: Set(Uuid::new_v4()),
            order_id: Set(order_id),
            variant_id: Set(Some(parse_uuid(&line.variant_id)?)),
            quantity: Set(line.quantity),
            unit_price_minor: Set(line.unit_price.amount_minor),
            line_total_minor: Set(line_total.amount_minor),
            currency: Set(line.unit_price.currency.as_str().to_owned()),
            product_name: Set(line.product_name.clone()),
            variant_sku: Set(line.variant_sku.clone()),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .map_err(|error| internal(&error))?;
    }
    Ok(())
}

pub(crate) async fn mark_cart_checked_out<C: ConnectionTrait>(
    db: &C,
    cart_uuid: Uuid,
    cart_id: &str,
    now: DateTimeWithTimeZone,
) -> Result<(), PersistenceError> {
    let mut active: cart::ActiveModel = cart::Entity::find_by_id(cart_uuid)
        .one(db)
        .await
        .map_err(|error| internal(&error))?
        .ok_or_else(|| PersistenceError::NotFound {
            entity: "cart",
            id: cart_id.to_owned(),
        })?
        .into();
    active.status = Set(CartStatus::CheckedOut.as_str().to_owned());
    active.updated_at = Set(now);
    active.update(db).await.map_err(|error| internal(&error))?;
    Ok(())
}

pub(crate) async fn find_order_by_key<C: ConnectionTrait>(
    db: &C,
    key: &str,
) -> Result<Option<Order>, PersistenceError> {
    let model = commerce_order::Entity::find()
        .filter(commerce_order::Column::IdempotencyKey.eq(key))
        .one(db)
        .await
        .map_err(|error| internal(&error))?;
    let Some(model) = model else {
        return Ok(None);
    };
    let lines = load_order_lines(db, model.id).await?;
    Ok(Some(order_from_models(model, lines)?))
}

pub(crate) async fn load_order<C: ConnectionTrait>(
    db: &C,
    order_id: Uuid,
) -> Result<Order, PersistenceError> {
    let model = commerce_order::Entity::find_by_id(order_id)
        .one(db)
        .await
        .map_err(|error| internal(&error))?
        .ok_or_else(|| PersistenceError::NotFound {
            entity: "order",
            id: order_id.to_string(),
        })?;
    let lines = load_order_lines(db, order_id).await?;
    order_from_models(model, lines)
}

pub(crate) async fn load_order_lines<C: ConnectionTrait>(
    db: &C,
    order_id: Uuid,
) -> Result<Vec<order_line::Model>, PersistenceError> {
    order_line::Entity::find()
        .filter(order_line::Column::OrderId.eq(order_id))
        .order_by_asc(order_line::Column::CreatedAt)
        .order_by_asc(order_line::Column::Id)
        .all(db)
        .await
        .map_err(|error| internal(&error))
}

#[cfg(test)]
mod helper_tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database};

    #[test]
    fn money_and_parse_uuid() {
        assert!(money(10, "no").is_err());
        assert_eq!(money(10, "EUR").unwrap().amount_minor, 10);
        assert!(parse_uuid("not-a-uuid").is_err());
        assert!(parse_uuid("33333333-3333-3333-3333-333333333331").is_ok());
    }

    #[test]
    fn internal_and_unique_helpers() {
        let err = internal(&DbErr::Custom("boom".into()));
        assert!(matches!(err, PersistenceError::Internal { .. }));
        assert!(is_unique_violation(&DbErr::Custom(
            "duplicate key value violates unique constraint".into()
        )));
        assert!(!is_unique_violation(&DbErr::Custom("other".into())));
    }

    fn sample_order(currency: &str) -> commerce_order::Model {
        let now = chrono::Utc::now().into();
        commerce_order::Model {
            id: Uuid::nil(),
            number: "RS-1".into(),
            customer_id: None,
            cart_id: None,
            state: "placed".into(),
            currency: currency.into(),
            items_total_minor: 100,
            total_minor: 100,
            idempotency_key: None,
            created_at: now,
            updated_at: now,
            placed_at: Some(now),
        }
    }

    #[test]
    fn order_from_models_rejects_bad_currency() {
        assert!(matches!(
            order_from_models(sample_order("ZZ"), vec![]),
            Err(PersistenceError::InvalidInput { .. })
        ));
        let now = chrono::Utc::now().into();
        let line = order_line::Model {
            id: Uuid::nil(),
            order_id: Uuid::nil(),
            variant_id: None,
            quantity: 1,
            unit_price_minor: 100,
            line_total_minor: 100,
            currency: "ZZ".into(),
            product_name: "Mug".into(),
            variant_sku: "MUG".into(),
            created_at: now,
            updated_at: now,
        };
        assert!(matches!(
            order_from_models(sample_order("EUR"), vec![line]),
            Err(PersistenceError::InvalidInput { .. })
        ));
    }

    #[tokio::test]
    async fn load_helpers_map_internal_when_order_tables_missing() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let mut options = ConnectOptions::new(url);
        options.max_connections(2);
        let db = Database::connect(options).await.expect("connect");
        db.execute_unprepared("SELECT pg_advisory_lock(874532)")
            .await
            .expect("lock");
        db.execute_unprepared("DROP SCHEMA public CASCADE; CREATE SCHEMA public")
            .await
            .expect("reset");
        crate::migrate(&db).await.expect("migrate");

        db.execute_unprepared("DROP TABLE order_line CASCADE")
            .await
            .expect("drop lines");
        let err = load_order_lines(&db, Uuid::nil()).await.expect_err("lines");
        assert!(matches!(err, PersistenceError::Internal { .. }));

        db.execute_unprepared(r#"DROP TABLE "order" CASCADE"#)
            .await
            .expect("drop order");
        let err = load_order(&db, Uuid::nil()).await.expect_err("order");
        assert!(matches!(err, PersistenceError::Internal { .. }));
        let err = find_order_by_key(&db, "any-key")
            .await
            .expect_err("find key");
        assert!(matches!(err, PersistenceError::Internal { .. }));

        db.execute_unprepared("DROP SCHEMA public CASCADE; CREATE SCHEMA public")
            .await
            .expect("cleanup");
        crate::migrate(&db).await.expect("migrate again");
        crate::seed_catalog(&db).await.expect("seed");

        let missing = Uuid::new_v4();
        let err = load_order(&db, missing).await.expect_err("order not found");
        assert!(matches!(
            err,
            PersistenceError::NotFound {
                entity: "order",
                ..
            }
        ));
        let err = mark_cart_checked_out(
            &db,
            missing,
            &missing.to_string(),
            chrono::Utc::now().into(),
        )
        .await
        .expect_err("cart not found");
        assert!(matches!(
            err,
            PersistenceError::NotFound { entity: "cart", .. }
        ));

        db.execute_unprepared("DROP TABLE cart CASCADE")
            .await
            .expect("drop cart");
        let err = mark_cart_checked_out(
            &db,
            missing,
            &missing.to_string(),
            chrono::Utc::now().into(),
        )
        .await
        .expect_err("cart table gone");
        assert!(matches!(err, PersistenceError::Internal { .. }));

        db.execute_unprepared("DROP SCHEMA public CASCADE; CREATE SCHEMA public")
            .await
            .expect("cleanup2");
        crate::migrate(&db).await.expect("migrate final");
        crate::seed_catalog(&db).await.expect("seed final");
        db.execute_unprepared("SELECT pg_advisory_unlock(874532)")
            .await
            .expect("unlock");
    }
}
