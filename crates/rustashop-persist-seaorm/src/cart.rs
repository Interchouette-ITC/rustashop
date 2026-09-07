//! `SeaORM` cart repository and line mutations.

use rustashop_domain::{
    Cart, CartLine, CartRepository, CartStatus, Currency, Money, ProductVariant,
};
use sea_orm::entity::prelude::*;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};
use serenade_contracts::PersistenceError;

use crate::SeaOrmCatalogRepository;
use crate::entities::{cart, cart_line, product, product_variant};
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

fn money(amount_minor: i64, currency: &str) -> Result<Money, PersistenceError> {
    let currency = Currency::new(currency).map_err(|error| PersistenceError::InvalidInput {
        message: error.to_string(),
    })?;
    Ok(Money::new(amount_minor, currency))
}

pub(crate) fn cart_from_models(
    model: cart::Model,
    lines: Vec<cart_line::Model>,
) -> Result<Cart, PersistenceError> {
    let currency =
        Currency::new(&model.currency).map_err(|error| PersistenceError::InvalidInput {
            message: error.to_string(),
        })?;
    let mut cart_lines = Vec::with_capacity(lines.len());
    for line in lines {
        cart_lines.push(CartLine {
            id: line.id.to_string(),
            cart_id: line.cart_id.to_string(),
            variant_id: line.variant_id.to_string(),
            quantity: line.quantity,
            unit_price: money(line.unit_price_minor, &line.currency)?,
            product_name: line.product_name,
            variant_sku: line.variant_sku,
        });
    }
    Ok(Cart {
        id: model.id.to_string(),
        customer_id: model.customer_id.map(|id| id.to_string()),
        token: model.token,
        currency,
        status: CartStatus::parse(&model.status).map_err(|error| {
            PersistenceError::InvalidInput {
                message: error.to_string(),
            }
        })?,
        lines: cart_lines,
    })
}

async fn load_lines<C: ConnectionTrait>(
    db: &C,
    cart_id: Uuid,
) -> Result<Vec<cart_line::Model>, PersistenceError> {
    cart_line::Entity::find()
        .filter(cart_line::Column::CartId.eq(cart_id))
        .order_by_asc(cart_line::Column::CreatedAt)
        .order_by_asc(cart_line::Column::Id)
        .all(db)
        .await
        .map_err(|error| internal(&error))
}

impl SeaOrmCatalogRepository {
    /// Creates an empty cart in `currency`.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError`] when the insert fails or the currency is invalid.
    pub async fn create_cart(&self, currency: &Currency) -> Result<Cart, PersistenceError> {
        let now = chrono_now();
        let model = cart::ActiveModel {
            id: Set(Uuid::new_v4()),
            customer_id: Set(None),
            token: Set(Uuid::new_v4().to_string()),
            currency: Set(currency.as_str().to_owned()),
            status: Set(CartStatus::Open.as_str().to_owned()),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.db)
        .await
        .map_err(|error| internal(&error))?;
        cart_from_models(model, Vec::new())
    }

    /// Loads a cart and its lines by id.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError`] on query failure.
    pub async fn find_cart_by_id(&self, id: &str) -> Result<Option<Cart>, PersistenceError> {
        let uuid = parse_uuid(id)?;
        let model = cart::Entity::find_by_id(uuid)
            .one(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        let Some(model) = model else {
            return Ok(None);
        };
        let lines = load_lines(&self.db, model.id).await?;
        Ok(Some(cart_from_models(model, lines)?))
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
        let uuid = parse_uuid(variant_id)?;
        let variant = product_variant::Entity::find_by_id(uuid)
            .one(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        let Some(variant) = variant else {
            return Ok(None);
        };
        let product = product::Entity::find_by_id(variant.product_id)
            .one(&self.db)
            .await
            .map_err(|error| internal(&error))?
            .ok_or_else(|| PersistenceError::NotFound {
                entity: "product",
                id: variant.product_id.to_string(),
            })?;
        Ok(Some((
            ProductVariant {
                id: variant.id.to_string(),
                product_id: variant.product_id.to_string(),
                sku: variant.sku,
                name: variant.name,
                price: money(variant.price_minor, &variant.currency)?,
                stock_quantity: variant.stock_quantity,
            },
            product.name,
        )))
    }

    /// Persists cart header and replaces all lines inside a transaction.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError`] when the transaction fails.
    pub async fn save_cart(&self, cart: &Cart) -> Result<(), PersistenceError> {
        let cart_id = parse_uuid(&cart.id)?;
        let txn = self.db.begin().await.map_err(|error| internal(&error))?;
        let existing = cart::Entity::find_by_id(cart_id)
            .one(&txn)
            .await
            .map_err(|error| internal(&error))?
            .ok_or_else(|| PersistenceError::NotFound {
                entity: "cart",
                id: cart.id.clone(),
            })?;
        let mut active: cart::ActiveModel = existing.into();
        active.customer_id = Set(match &cart.customer_id {
            Some(id) => Some(parse_uuid(id)?),
            None => None,
        });
        active.currency = Set(cart.currency.as_str().to_owned());
        active.status = Set(cart.status.as_str().to_owned());
        active.updated_at = Set(chrono_now());
        active
            .update(&txn)
            .await
            .map_err(|error| internal(&error))?;

        cart_line::Entity::delete_many()
            .filter(cart_line::Column::CartId.eq(cart_id))
            .exec(&txn)
            .await
            .map_err(|error| internal(&error))?;

        let now = chrono_now();
        for line in &cart.lines {
            let line_id = if line.id.is_empty() {
                Uuid::new_v4()
            } else {
                parse_uuid(&line.id)?
            };
            cart_line::ActiveModel {
                id: Set(line_id),
                cart_id: Set(cart_id),
                variant_id: Set(parse_uuid(&line.variant_id)?),
                quantity: Set(line.quantity),
                unit_price_minor: Set(line.unit_price.amount_minor),
                currency: Set(line.unit_price.currency.as_str().to_owned()),
                product_name: Set(line.product_name.clone()),
                variant_sku: Set(line.variant_sku.clone()),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(&txn)
            .await
            .map_err(|error| internal(&error))?;
        }
        txn.commit().await.map_err(|error| internal(&error))?;
        Ok(())
    }
}

fn chrono_now() -> DateTimeWithTimeZone {
    chrono::Utc::now().into()
}

impl CartRepository for SeaOrmCatalogRepository {
    type Error = PersistenceError;
    type Id = String;
    type Cart = Cart;

    async fn find_by_token(&self, token: &str) -> Result<Option<Self::Cart>, Self::Error> {
        let model = cart::Entity::find()
            .filter(cart::Column::Token.eq(token))
            .one(&self.db)
            .await
            .map_err(|error| internal(&error))?;
        let Some(model) = model else {
            return Ok(None);
        };
        let lines = load_lines(&self.db, model.id).await?;
        Ok(Some(cart_from_models(model, lines)?))
    }

    async fn save(&self, cart: &Self::Cart) -> Result<(), Self::Error> {
        self.save_cart(cart).await
    }

    async fn delete(&self, id: &Self::Id) -> Result<(), Self::Error> {
        let uuid = parse_uuid(id)?;
        cart::Entity::delete_by_id(uuid)
            .exec(&self.db)
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
    fn seaorm_catalog_implements_cart_repository() {
        assert_cart_repo::<SeaOrmCatalogRepository>();
    }

    fn sample_cart(currency: &str, status: &str) -> cart::Model {
        let now = chrono::Utc::now().into();
        cart::Model {
            id: Uuid::nil(),
            customer_id: None,
            token: "tok".into(),
            currency: currency.into(),
            status: status.into(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn cart_from_models_rejects_bad_currency_and_status() {
        assert!(matches!(
            cart_from_models(sample_cart("ZZ", "open"), vec![]),
            Err(PersistenceError::InvalidInput { .. })
        ));
        assert!(matches!(
            cart_from_models(sample_cart("EUR", "nope"), vec![]),
            Err(PersistenceError::InvalidInput { .. })
        ));
        let now = chrono::Utc::now().into();
        let line = cart_line::Model {
            id: Uuid::nil(),
            cart_id: Uuid::nil(),
            variant_id: Uuid::nil(),
            quantity: 1,
            unit_price_minor: 100,
            currency: "ZZ".into(),
            product_name: "Mug".into(),
            variant_sku: "MUG".into(),
            created_at: now,
            updated_at: now,
        };
        assert!(matches!(
            cart_from_models(sample_cart("EUR", "open"), vec![line]),
            Err(PersistenceError::InvalidInput { .. })
        ));
        let ok = cart_from_models(sample_cart("EUR", "open"), vec![]).expect("ok");
        assert_eq!(ok.currency.as_str(), "EUR");
    }
}
