//! Cart handlers (JSON via Serenade front; utoipa path items for `OpenAPI`).

use rustashop_domain::{Cart, CartLine, Currency};
use rustashop_persist::CatalogRepository;
use serde::{Deserialize, Serialize};
use serenade_http::Response;
use utoipa::ToSchema;

use crate::error::{ApiError, ErrorBody, api_error_json_response, json_response};
use crate::realtime::{CartHub, CartRealtimeEvent};
use crate::request_param::{ensure_request_param, ensure_request_param_opt};

/// Body for `POST /v1/carts`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCartRequest {
    /// ISO currency for the cart (default `EUR`).
    pub currency: Option<String>,
}

/// Body for `POST /v1/carts/{id}/lines`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddCartLineRequest {
    /// Variant to add.
    pub variant_id: String,
    /// Quantity greater than zero.
    pub quantity: i32,
}

/// Body for `PATCH /v1/carts/{id}/lines/{line_id}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCartLineRequest {
    /// Replacement quantity greater than zero.
    pub quantity: i32,
}

/// Money JSON for cart responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct MoneyResponse {
    /// Amount in minor units.
    pub amount_minor: i64,
    /// ISO currency code.
    pub currency: String,
}

/// Cart line JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct CartLineResponse {
    /// Line id.
    pub id: String,
    /// Variant id.
    pub variant_id: String,
    /// Quantity.
    pub quantity: i32,
    /// Snapshotted unit price.
    pub unit_price: MoneyResponse,
    /// Line total (`unit_price × quantity`).
    pub line_total: MoneyResponse,
    /// Snapshotted product name.
    pub product_name: String,
    /// Snapshotted SKU.
    pub variant_sku: String,
}

/// Cart JSON including recalculated totals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct CartResponse {
    /// Cart id.
    pub id: String,
    /// Optional customer id.
    pub customer_id: Option<String>,
    /// Session token.
    pub token: String,
    /// Cart status (`open` or `checked_out`).
    pub status: String,
    /// Cart currency.
    pub currency: String,
    /// Cart lines.
    pub lines: Vec<CartLineResponse>,
    /// Sum of line totals.
    pub items_total: MoneyResponse,
}

impl CartResponse {
    fn try_from_cart(cart: Cart) -> Result<Self, ApiError> {
        let items_total = cart
            .items_total()
            .map_err(|error| ApiError::from_domain(&error))?;
        let mut lines = Vec::with_capacity(cart.lines.len());
        for line in cart.lines {
            let line_total = line
                .line_total()
                .map_err(|error| ApiError::from_domain(&error))?;
            lines.push(CartLineResponse {
                id: line.id,
                variant_id: line.variant_id,
                quantity: line.quantity,
                unit_price: MoneyResponse {
                    amount_minor: line.unit_price.amount_minor,
                    currency: line.unit_price.currency.as_str().to_owned(),
                },
                line_total: MoneyResponse {
                    amount_minor: line_total.amount_minor,
                    currency: line_total.currency.as_str().to_owned(),
                },
                product_name: line.product_name,
                variant_sku: line.variant_sku,
            });
        }
        Ok(Self {
            id: cart.id,
            customer_id: cart.customer_id,
            token: cart.token,
            status: cart.status.as_str().to_owned(),
            currency: cart.currency.as_str().to_owned(),
            lines,
            items_total: MoneyResponse {
                amount_minor: items_total.amount_minor,
                currency: items_total.currency.as_str().to_owned(),
            },
        })
    }
}

fn parse_json_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|error| ApiError::Unprocessable(error.to_string()))
}

fn respond_cart(
    status: u16,
    hub: Option<&CartHub>,
    result: Result<CartResponse, ApiError>,
) -> Response {
    match result {
        Ok(body) => {
            if let Some(hub) = hub {
                hub.publish(&CartRealtimeEvent::updated(body.clone()));
            }
            json_response(status, &body)
        }
        Err(error) => api_error_json_response(&error),
    }
}

async fn reload_cart(store: &CatalogRepository, id: &str) -> Result<CartResponse, ApiError> {
    let cart = store
        .find_cart_by_id(id)
        .await
        .map_err(|error| ApiError::from_persist(&error))?
        .ok_or(ApiError::NotFound)?;
    CartResponse::try_from_cart(cart)
}

async fn save_then_reload(
    store: &CatalogRepository,
    cart: &Cart,
    cart_id: &str,
) -> Result<CartResponse, ApiError> {
    store
        .save_cart(cart)
        .await
        .map_err(|error| ApiError::from_persist(&error))?;
    reload_cart(store, cart_id).await
}

/// Creates an empty cart as a Serenade JSON [`Response`].
pub async fn create_cart_response(
    catalog: &CatalogRepository,
    hub: Option<&CartHub>,
    body: &[u8],
) -> Response {
    let request = match parse_json_body::<CreateCartRequest>(body) {
        Ok(request) => request,
        Err(error) => return api_error_json_response(&error),
    };
    let code = match ensure_request_param_opt(request.currency.as_deref()) {
        Ok(code) => code.unwrap_or("EUR"),
        Err(error) => return api_error_json_response(&error),
    };
    let currency = match Currency::new(code) {
        Ok(currency) => currency,
        Err(error) => return api_error_json_response(&ApiError::from_domain(&error)),
    };
    match catalog.create_cart(&currency).await {
        Ok(cart) => respond_cart(201, hub, CartResponse::try_from_cart(cart)),
        Err(error) => api_error_json_response(&ApiError::from_persist(&error)),
    }
}

/// Returns one cart by id as a Serenade JSON [`Response`].
pub async fn get_cart_response(catalog: &CatalogRepository, id: &str) -> Response {
    if let Err(error) = ensure_request_param(id) {
        return api_error_json_response(&error);
    }
    // Reads do not push realtime events.
    match reload_cart(catalog, id).await {
        Ok(body) => json_response(200, &body),
        Err(error) => api_error_json_response(&error),
    }
}

/// Adds a line (merges quantity when the variant is already present).
pub async fn add_cart_line_response(
    catalog: &CatalogRepository,
    hub: Option<&CartHub>,
    cart_id: &str,
    body: &[u8],
) -> Response {
    if let Err(error) = ensure_request_param(cart_id) {
        return api_error_json_response(&error);
    }
    let request = match parse_json_body::<AddCartLineRequest>(body) {
        Ok(request) => request,
        Err(error) => return api_error_json_response(&error),
    };
    if let Err(error) = ensure_request_param(&request.variant_id) {
        return api_error_json_response(&error);
    }
    let mut cart = match catalog.find_cart_by_id(cart_id).await {
        Ok(Some(cart)) => cart,
        Ok(None) => return api_error_json_response(&ApiError::NotFound),
        Err(error) => return api_error_json_response(&ApiError::from_persist(&error)),
    };
    let (variant, product_name) = match catalog.find_variant_for_cart(&request.variant_id).await {
        Ok(Some(pair)) => pair,
        Ok(None) => return api_error_json_response(&ApiError::NotFound),
        Err(error) => return api_error_json_response(&ApiError::from_persist(&error)),
    };
    let line = match CartLine::from_variant(
        String::new(),
        cart.id.clone(),
        &variant,
        product_name,
        request.quantity,
    ) {
        Ok(line) => line,
        Err(error) => return api_error_json_response(&ApiError::from_domain(&error)),
    };
    if let Err(error) = cart.upsert_line(line) {
        return api_error_json_response(&ApiError::from_domain(&error));
    }
    respond_cart(200, hub, save_then_reload(catalog, &cart, cart_id).await)
}

/// Updates a line quantity.
pub async fn update_cart_line_response(
    catalog: &CatalogRepository,
    hub: Option<&CartHub>,
    cart_id: &str,
    line_id: &str,
    body: &[u8],
) -> Response {
    if let Err(error) = ensure_request_param(cart_id) {
        return api_error_json_response(&error);
    }
    if let Err(error) = ensure_request_param(line_id) {
        return api_error_json_response(&error);
    }
    let request = match parse_json_body::<UpdateCartLineRequest>(body) {
        Ok(request) => request,
        Err(error) => return api_error_json_response(&error),
    };
    let mut cart = match catalog.find_cart_by_id(cart_id).await {
        Ok(Some(cart)) => cart,
        Ok(None) => return api_error_json_response(&ApiError::NotFound),
        Err(error) => return api_error_json_response(&ApiError::from_persist(&error)),
    };
    if let Err(error) = cart.update_line_quantity(line_id, request.quantity) {
        return api_error_json_response(&ApiError::from_domain(&error));
    }
    respond_cart(200, hub, save_then_reload(catalog, &cart, cart_id).await)
}

/// Removes a line.
pub async fn delete_cart_line_response(
    catalog: &CatalogRepository,
    hub: Option<&CartHub>,
    cart_id: &str,
    line_id: &str,
) -> Response {
    if let Err(error) = ensure_request_param(cart_id) {
        return api_error_json_response(&error);
    }
    if let Err(error) = ensure_request_param(line_id) {
        return api_error_json_response(&error);
    }
    let mut cart = match catalog.find_cart_by_id(cart_id).await {
        Ok(Some(cart)) => cart,
        Ok(None) => return api_error_json_response(&ApiError::NotFound),
        Err(error) => return api_error_json_response(&ApiError::from_persist(&error)),
    };
    if let Err(error) = cart.remove_line(line_id) {
        return api_error_json_response(&ApiError::from_domain(&error));
    }
    respond_cart(200, hub, save_then_reload(catalog, &cart, cart_id).await)
}

/// `POST /v1/carts` `OpenAPI` path (served by the Serenade HTTP front controller).
#[utoipa::path(
    post,
    path = "/v1/carts",
    request_body = CreateCartRequest,
    responses(
        (status = 201, description = "Cart created", body = CartResponse),
        (status = 422, description = "Invalid currency", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn create_cart() {}

/// `GET /v1/carts/{id}` `OpenAPI` path (served by the Serenade HTTP front controller).
#[utoipa::path(
    get,
    path = "/v1/carts/{id}",
    params(("id" = String, Path, description = "Cart id")),
    responses(
        (status = 200, description = "Cart", body = CartResponse),
        (status = 404, description = "Unknown id", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn get_cart() {}

/// `POST /v1/carts/{id}/lines` `OpenAPI` path (served by the Serenade HTTP front controller).
#[utoipa::path(
    post,
    path = "/v1/carts/{id}/lines",
    params(("id" = String, Path, description = "Cart id")),
    request_body = AddCartLineRequest,
    responses(
        (status = 200, description = "Cart after add", body = CartResponse),
        (status = 404, description = "Unknown cart or variant", body = ErrorBody),
        (status = 422, description = "Invalid quantity or currency", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn add_cart_line() {}

/// `PATCH /v1/carts/{id}/lines/{line_id}` `OpenAPI` path (served by the Serenade HTTP front controller).
#[utoipa::path(
    patch,
    path = "/v1/carts/{id}/lines/{line_id}",
    params(
        ("id" = String, Path, description = "Cart id"),
        ("line_id" = String, Path, description = "Line id")
    ),
    request_body = UpdateCartLineRequest,
    responses(
        (status = 200, description = "Cart after update", body = CartResponse),
        (status = 404, description = "Unknown cart or line", body = ErrorBody),
        (status = 422, description = "Invalid quantity", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn update_cart_line() {}

/// `DELETE /v1/carts/{id}/lines/{line_id}` `OpenAPI` path (served by the Serenade HTTP front controller).
#[utoipa::path(
    delete,
    path = "/v1/carts/{id}/lines/{line_id}",
    params(
        ("id" = String, Path, description = "Cart id"),
        ("line_id" = String, Path, description = "Line id")
    ),
    responses(
        (status = 200, description = "Cart after delete", body = CartResponse),
        (status = 404, description = "Unknown cart or line", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn delete_cart_line() {}

#[cfg(test)]
mod stub_tests {
    use super::*;

    #[test]
    fn openapi_stubs_are_callable() {
        create_cart();
        get_cart();
        add_cart_line();
        update_cart_line();
        delete_cart_line();
    }

    #[test]
    fn respond_cart_maps_errors() {
        let response = respond_cart(201, None, Err(ApiError::NotFound));
        assert_eq!(response.status(), 404);
    }

    #[test]
    fn try_from_cart_maps_currency_mismatch() {
        use rustashop_domain::{CartStatus, Money};

        let eur = Currency::new("EUR").expect("EUR");
        let usd = Currency::new("USD").expect("USD");
        let cart = Cart {
            id: "c".into(),
            customer_id: None,
            token: "t".into(),
            status: CartStatus::Open,
            currency: eur,
            lines: vec![CartLine {
                id: "l".into(),
                cart_id: "c".into(),
                variant_id: "v".into(),
                quantity: 1,
                unit_price: Money::new(100, usd),
                product_name: "p".into(),
                variant_sku: "s".into(),
            }],
        };
        assert!(CartResponse::try_from_cart(cart).is_err());
    }
}

#[cfg(all(test, feature = "persist-sqlx"))]
mod cart_response_tests {
    use super::*;
    use rustashop_persist_sqlx::{SqlxCatalogRepository, migrate, seed_catalog};
    use sqlx::postgres::PgPoolOptions;

    const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";
    const MUG_VARIANT: &str = "33333333-3333-3333-3333-333333333332";
    const MISSING_CART: &str = "11111111-1111-1111-1111-111111111111";
    // Shared with other rustashop-api lib tests that reset `public`.
    const SCHEMA_LOCK: i64 = 874_521;

    async fn seeded() -> (SqlxCatalogRepository, sqlx::PgPool) {
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
        (SqlxCatalogRepository::new(pool.clone()), pool)
    }

    #[tokio::test]
    async fn covers_request_validation_errors() {
        let (catalog, _pool) = seeded().await;

        assert_eq!(
            create_cart_response(&catalog, None, b"not-json")
                .await
                .status(),
            422
        );
        assert_eq!(
            create_cart_response(&catalog, None, br#"{"currency":"a\u0000b"}"#)
                .await
                .status(),
            422
        );
        assert_eq!(
            create_cart_response(&catalog, None, br#"{"currency":"EURO"}"#)
                .await
                .status(),
            422
        );
        assert_eq!(get_cart_response(&catalog, "a\0b").await.status(), 422);
        assert_eq!(
            get_cart_response(&catalog, MISSING_CART).await.status(),
            404
        );

        let created = create_cart_response(&catalog, None, br#"{"currency":"EUR"}"#).await;
        assert_eq!(created.status(), 201);
        let cart: CartResponse = serde_json::from_slice(created.body()).expect("cart");

        assert_eq!(
            add_cart_line_response(&catalog, None, &cart.id, b"{")
                .await
                .status(),
            422
        );
        assert_eq!(
            add_cart_line_response(
                &catalog,
                None,
                "a\0b",
                format!(r#"{{"variant_id":"{HOODIE_VARIANT}","quantity":1}}"#).as_bytes(),
            )
            .await
            .status(),
            422
        );
        assert_eq!(
            add_cart_line_response(
                &catalog,
                None,
                &cart.id,
                br#"{"variant_id":"a\u0000b","quantity":1}"#,
            )
            .await
            .status(),
            422
        );
        assert_eq!(
            add_cart_line_response(
                &catalog,
                None,
                MISSING_CART,
                format!(r#"{{"variant_id":"{HOODIE_VARIANT}","quantity":1}}"#).as_bytes(),
            )
            .await
            .status(),
            404
        );
        assert_eq!(
            add_cart_line_response(
                &catalog,
                None,
                &cart.id,
                br#"{"variant_id":"99999999-9999-9999-9999-999999999999","quantity":1}"#,
            )
            .await
            .status(),
            404
        );
        assert_eq!(
            add_cart_line_response(
                &catalog,
                None,
                &cart.id,
                format!(r#"{{"variant_id":"{HOODIE_VARIANT}","quantity":0}}"#).as_bytes(),
            )
            .await
            .status(),
            422
        );
    }

    #[tokio::test]
    async fn covers_line_update_delete_validation() {
        let (catalog, _pool) = seeded().await;
        let created = create_cart_response(&catalog, None, br#"{"currency":"EUR"}"#).await;
        let cart: CartResponse = serde_json::from_slice(created.body()).expect("cart");

        assert_eq!(
            add_cart_line_response(
                &catalog,
                None,
                &cart.id,
                format!(r#"{{"variant_id":"{HOODIE_VARIANT}","quantity":2000000000}}"#).as_bytes(),
            )
            .await
            .status(),
            200
        );
        assert_eq!(
            add_cart_line_response(
                &catalog,
                None,
                &cart.id,
                format!(r#"{{"variant_id":"{HOODIE_VARIANT}","quantity":2000000000}}"#).as_bytes(),
            )
            .await
            .status(),
            422
        );

        let with_line: CartResponse = serde_json::from_slice(
            add_cart_line_response(
                &catalog,
                None,
                &cart.id,
                format!(r#"{{"variant_id":"{MUG_VARIANT}","quantity":1}}"#).as_bytes(),
            )
            .await
            .body(),
        )
        .expect("mug");
        let line_id = with_line
            .lines
            .iter()
            .find(|line| line.variant_id == MUG_VARIANT)
            .expect("mug line")
            .id
            .clone();

        assert_eq!(
            update_cart_line_response(&catalog, None, "a\0b", &line_id, br#"{"quantity":2}"#)
                .await
                .status(),
            422
        );
        assert_eq!(
            update_cart_line_response(&catalog, None, &cart.id, "a\0b", br#"{"quantity":2}"#)
                .await
                .status(),
            422
        );
        assert_eq!(
            update_cart_line_response(&catalog, None, &cart.id, &line_id, b"x")
                .await
                .status(),
            422
        );
        assert_eq!(
            update_cart_line_response(&catalog, None, MISSING_CART, &line_id, br#"{"quantity":2}"#)
                .await
                .status(),
            404
        );
        assert_eq!(
            update_cart_line_response(&catalog, None, &cart.id, "no-line", br#"{"quantity":2}"#)
                .await
                .status(),
            404
        );
        assert_eq!(
            delete_cart_line_response(&catalog, None, "a\0b", &line_id)
                .await
                .status(),
            422
        );
        assert_eq!(
            delete_cart_line_response(&catalog, None, &cart.id, "a\0b")
                .await
                .status(),
            422
        );
        assert_eq!(
            delete_cart_line_response(&catalog, None, MISSING_CART, &line_id)
                .await
                .status(),
            404
        );
    }

    #[tokio::test]
    async fn covers_save_and_closed_pool_errors() {
        let (catalog, pool) = seeded().await;
        let created = create_cart_response(&catalog, None, br#"{"currency":"EUR"}"#).await;
        let cart: CartResponse = serde_json::from_slice(created.body()).expect("cart");
        let added = add_cart_line_response(
            &catalog,
            None,
            &cart.id,
            format!(r#"{{"variant_id":"{MUG_VARIANT}","quantity":1}}"#).as_bytes(),
        )
        .await;
        let with_line: CartResponse = serde_json::from_slice(added.body()).expect("line");
        let line_id = with_line.lines[0].id.clone();

        // SELECT still works; writes fail → save_then_reload Err arm.
        sqlx::query(
            "CREATE OR REPLACE FUNCTION rustashop_block_cart_line() RETURNS trigger AS $$\
             BEGIN RAISE EXCEPTION 'blocked'; END; $$ LANGUAGE plpgsql",
        )
        .execute(&pool)
        .await
        .expect("fn");
        sqlx::query(
            "CREATE TRIGGER rustashop_block_cart_line_trg \
             BEFORE INSERT OR UPDATE OR DELETE ON cart_line \
             FOR EACH ROW EXECUTE FUNCTION rustashop_block_cart_line()",
        )
        .execute(&pool)
        .await
        .expect("trigger");
        assert_eq!(
            update_cart_line_response(&catalog, None, &cart.id, &line_id, br#"{"quantity":3}"#)
                .await
                .status(),
            500
        );

        sqlx::query("DROP TABLE product_variant CASCADE")
            .execute(&pool)
            .await
            .expect("drop variants");
        assert_eq!(
            add_cart_line_response(
                &catalog,
                None,
                &cart.id,
                format!(r#"{{"variant_id":"{HOODIE_VARIANT}","quantity":1}}"#).as_bytes(),
            )
            .await
            .status(),
            500
        );

        pool.close().await;
        assert_eq!(
            create_cart_response(&catalog, None, br"{}").await.status(),
            500
        );
        assert_eq!(
            add_cart_line_response(
                &catalog,
                None,
                &cart.id,
                format!(r#"{{"variant_id":"{HOODIE_VARIANT}","quantity":1}}"#).as_bytes(),
            )
            .await
            .status(),
            500
        );
        assert_eq!(
            update_cart_line_response(&catalog, None, &cart.id, &line_id, br#"{"quantity":1}"#)
                .await
                .status(),
            500
        );
        assert_eq!(
            delete_cart_line_response(&catalog, None, &cart.id, &line_id)
                .await
                .status(),
            500
        );
    }
}
