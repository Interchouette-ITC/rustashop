//! Checkout handler (JSON via Serenade front; utoipa path items for `OpenAPI`).

use rustashop_domain::Order;
use rustashop_persist::CatalogRepository;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use serenade_http::Response;
use serenade_validator::{
    Constraint, ConstraintViolationList, Length, NotBlank, Validatable, Validator,
};
use tracing::warn;
use utoipa::ToSchema;

use crate::carts::MoneyResponse;
use crate::error::{ApiError, ErrorBody, api_error_json_response, json_response};
use crate::order_mail::OrderMailer;
use crate::request_param::{ensure_request_param, ensure_request_param_opt};
use crate::request_validate::validate_request;

/// Body for `POST /v1/checkout`.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({
    "cart_id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
    "email": "buyer@example.test"
}))]
pub struct CheckoutRequest {
    /// Cart to convert into an order.
    #[schema(example = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")]
    pub cart_id: String,
    /// Optional confirmation recipient (`serenade-mailer` when set).
    #[schema(example = "buyer@example.test")]
    pub email: Option<String>,
}

impl Validatable for CheckoutRequest {
    fn validate(&self, validator: &dyn Validator) -> ConstraintViolationList {
        let mut list = validator.validate_value(
            &self.cart_id,
            "cart_id",
            &[&NotBlank as &dyn Constraint, &Length::new(1, 128)],
        );
        if let Some(email) = &self.email {
            let email_violations = validator.validate_value(
                email,
                "email",
                &[&NotBlank as &dyn Constraint, &Length::new(3, 254)],
            );
            for violation in email_violations.as_slice() {
                list.add(violation.clone());
            }
        }
        list
    }
}

/// Order line JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct OrderLineResponse {
    /// Line id.
    pub id: String,
    /// Variant id when still known.
    pub variant_id: Option<String>,
    /// Quantity.
    pub quantity: i32,
    /// Snapshotted unit price.
    pub unit_price: MoneyResponse,
    /// Snapshotted line total.
    pub line_total: MoneyResponse,
    /// Snapshotted product name.
    pub product_name: String,
    /// Snapshotted SKU.
    pub variant_sku: String,
}

/// Order JSON returned by checkout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct OrderResponse {
    /// Order id.
    pub id: String,
    /// Human-readable number.
    pub number: String,
    /// Source cart id.
    pub cart_id: Option<String>,
    /// Fulfillment state.
    pub state: String,
    /// Payment status (`pending` at checkout; no provider attached yet).
    pub payment_status: String,
    /// Order currency.
    pub currency: String,
    /// Sum of line totals.
    pub items_total: MoneyResponse,
    /// Payable total.
    pub total: MoneyResponse,
    /// Order lines.
    pub lines: Vec<OrderLineResponse>,
}

impl From<Order> for OrderResponse {
    fn from(order: Order) -> Self {
        Self {
            id: order.id,
            number: order.number,
            cart_id: order.cart_id,
            state: order.state,
            payment_status: order.payment_status,
            currency: order.currency.as_str().to_owned(),
            items_total: MoneyResponse {
                amount_minor: order.items_total.amount_minor,
                currency: order.items_total.currency.as_str().to_owned(),
            },
            total: MoneyResponse {
                amount_minor: order.total.amount_minor,
                currency: order.total.currency.as_str().to_owned(),
            },
            lines: order
                .lines
                .into_iter()
                .map(|line| OrderLineResponse {
                    id: line.id,
                    variant_id: line.variant_id,
                    quantity: line.quantity,
                    unit_price: MoneyResponse {
                        amount_minor: line.unit_price.amount_minor,
                        currency: line.unit_price.currency.as_str().to_owned(),
                    },
                    line_total: MoneyResponse {
                        amount_minor: line.line_total.amount_minor,
                        currency: line.line_total.currency.as_str().to_owned(),
                    },
                    product_name: line.product_name,
                    variant_sku: line.variant_sku,
                })
                .collect(),
        }
    }
}

fn parse_json_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|error| ApiError::Unprocessable(error.to_string()))
}

/// Reads optional `Idempotency-Key` header (case-insensitive via Serenade headers).
#[must_use]
pub fn idempotency_key_from_headers(headers: &serenade_http::Headers) -> Option<String> {
    headers
        .get("idempotency-key")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Converts a cart into a placed order as a Serenade JSON [`Response`].
pub async fn place_order_response(
    catalog: &CatalogRepository,
    mailer: Option<&OrderMailer>,
    body: &[u8],
    idempotency_key: Option<&str>,
) -> Response {
    let request = match parse_json_body::<CheckoutRequest>(body) {
        Ok(request) => request,
        Err(error) => return api_error_json_response(&error),
    };
    if let Err(error) = validate_request(&request) {
        return api_error_json_response(&error);
    }
    if let Err(error) = ensure_request_param(&request.cart_id) {
        return api_error_json_response(&error);
    }
    if let Err(error) = ensure_request_param_opt(request.email.as_deref()) {
        return api_error_json_response(&error);
    }
    let key = match ensure_request_param_opt(idempotency_key) {
        Ok(key) => key,
        Err(error) => return api_error_json_response(&error),
    };
    match catalog.checkout_cart(&request.cart_id, key).await {
        Ok(order) => {
            let response = OrderResponse::from(order);
            if let (Some(mailer), Some(email)) = (mailer, request.email.as_deref())
                && let Err(error) = mailer.send_order_confirmation(&response, email)
            {
                warn!(error = %error, "order confirmation mail failed");
            }
            json_response(201, &response)
        }
        Err(error) => api_error_json_response(&ApiError::from_persist(&error)),
    }
}

/// `POST /v1/checkout` `OpenAPI` path (served by the Serenade HTTP front controller).
#[utoipa::path(
    post,
    path = "/v1/checkout",
    tag = "checkout",
    request_body = CheckoutRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "Replay token")),
    responses(
        (status = 201, description = "Order placed", body = OrderResponse,
            example = json!({
                "id": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
                "number": "RS-1001",
                "cart_id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                "state": "placed",
                "payment_status": "pending",
                "currency": "EUR",
                "items_total": {"amount_minor": 4500, "currency": "EUR"},
                "total": {"amount_minor": 4500, "currency": "EUR"},
                "lines": [{
                    "id": "cccccccc-cccc-cccc-cccc-cccccccccccc",
                    "variant_id": "33333333-3333-3333-3333-333333333331",
                    "quantity": 1,
                    "unit_price": {"amount_minor": 4500, "currency": "EUR"},
                    "line_total": {"amount_minor": 4500, "currency": "EUR"},
                    "product_name": "Hoodie",
                    "variant_sku": "HOODIE-S"
                }]
            })),
        (status = 404, description = "Unknown cart", body = ErrorBody),
        (status = 409, description = "Cart already checked out", body = ErrorBody),
        (status = 422, description = "Empty cart", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn place_order() {}

#[cfg(test)]
mod stub_tests {
    use super::*;
    use serenade_http::Headers;

    #[test]
    fn openapi_stub_is_callable() {
        place_order();
    }

    #[test]
    fn reads_idempotency_header() {
        let mut headers = Headers::new();
        headers.insert("Idempotency-Key", "  abc  ");
        assert_eq!(
            idempotency_key_from_headers(&headers).as_deref(),
            Some("abc")
        );
        assert_eq!(idempotency_key_from_headers(&Headers::new()), None);
        let mut blank = Headers::new();
        blank.insert("idempotency-key", "   ");
        assert_eq!(idempotency_key_from_headers(&blank), None);
    }
}

#[cfg(all(test, feature = "persist-sqlx"))]
mod checkout_response_tests {
    use super::*;
    use rustashop_persist_sqlx::{SqlxCatalogRepository, migrate, seed_catalog};
    use sqlx::postgres::PgPoolOptions;

    // Shared with other rustashop-api lib tests that reset `public`.
    const SCHEMA_LOCK: i64 = 874_521;

    #[tokio::test]
    async fn covers_bad_body_and_missing_cart() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(SCHEMA_LOCK)
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
        migrate(&pool).await.expect("migrate");
        seed_catalog(&pool).await.expect("seed");
        let catalog = SqlxCatalogRepository::new(pool);

        let bad = place_order_response(&catalog, None, b"{}", None).await;
        assert_eq!(bad.status(), 422);

        let missing = place_order_response(
            &catalog,
            None,
            br#"{"cart_id":"11111111-1111-1111-1111-111111111111"}"#,
            None,
        )
        .await;
        assert_eq!(missing.status(), 404);

        let nul =
            place_order_response(&catalog, None, br#"{"cart_id":"a\u0000b"}"#, Some("k")).await;
        assert_eq!(nul.status(), 422);

        let nul_key = place_order_response(
            &catalog,
            None,
            br#"{"cart_id":"11111111-1111-1111-1111-111111111111"}"#,
            Some("a\0b"),
        )
        .await;
        assert_eq!(nul_key.status(), 422);

        let blank_email = place_order_response(
            &catalog,
            None,
            br#"{"cart_id":"11111111-1111-1111-1111-111111111111","email":"  "}"#,
            None,
        )
        .await;
        assert_eq!(blank_email.status(), 422);
    }

    #[tokio::test]
    async fn places_order_and_sends_confirmation_mail() {
        use std::sync::{Arc, Mutex};

        use crate::carts::{add_cart_line_response, create_cart_response};
        use crate::order_mail::OrderMailer;

        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(SCHEMA_LOCK)
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
        migrate(&pool).await.expect("migrate");
        seed_catalog(&pool).await.expect("seed");
        let catalog = SqlxCatalogRepository::new(pool);

        let created = create_cart_response(&catalog, None, br#"{"currency":"EUR"}"#).await;
        assert_eq!(created.status(), 201);
        let cart: crate::carts::CartResponse =
            serde_json::from_slice(created.body()).expect("cart");
        let added = add_cart_line_response(
            &catalog,
            None,
            &cart.id,
            br#"{"variant_id":"33333333-3333-3333-3333-333333333331","quantity":1}"#,
        )
        .await;
        assert_eq!(added.status(), 200);

        let outbox = Arc::new(Mutex::new(Vec::new()));
        let mailer = OrderMailer::recording(Arc::clone(&outbox));
        let body = format!(
            r#"{{"cart_id":"{}","email":"buyer@example.test"}}"#,
            cart.id
        );
        let placed = place_order_response(&catalog, Some(&mailer), body.as_bytes(), None).await;
        assert_eq!(placed.status(), 201);
        let (len, subject) = {
            let sent = outbox.lock().expect("lock");
            (sent.len(), sent[0].subject_line().to_owned())
        };
        assert_eq!(len, 1);
        assert!(subject.contains("confirmed"));
    }
}
