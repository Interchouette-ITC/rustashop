//! Admin order list and status PATCH (JSON via Serenade front; utoipa path items for `OpenAPI`).

use rustashop_domain::OrderState;
use rustashop_persist::CatalogRepository;
use serde::{Deserialize, Serialize};
use serenade_contracts::PageRequest;
use serenade_http::Response;
use utoipa::{IntoParams, ToSchema};

use crate::admin_auth::AdminAuthConfig;
use crate::checkout::OrderResponse;
use crate::error::{ApiError, ErrorBody, api_error_json_response, json_response};
use crate::request_param::ensure_request_param;

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 100;

/// Query string for admin order list.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListOrdersQuery {
    /// Maximum rows (capped).
    pub limit: Option<u32>,
    /// Rows to skip.
    pub offset: Option<u32>,
}

impl ListOrdersQuery {
    /// Parses `limit` / `offset` from a raw query string (`a=1&b=2`).
    #[must_use]
    pub fn from_query_string(query: Option<&str>) -> Self {
        let Some(query) = query.filter(|value| !value.is_empty()) else {
            return Self::default();
        };
        let mut limit = None;
        let mut offset = None;
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or("");
            if key.is_empty() {
                continue;
            }
            let value = parts.next().unwrap_or("");
            match key {
                "limit" => limit = value.parse().ok(),
                "offset" => offset = value.parse().ok(),
                _ => {}
            }
        }
        Self { limit, offset }
    }
}

/// Paginated admin order list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct OrderListResponse {
    /// Orders newest first.
    pub items: Vec<OrderResponse>,
}

/// Body for admin order status PATCH.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchOrderStatusRequest {
    /// Fulfillment status: `placed`, `paid`, `shipped`, or `cancelled`.
    pub status: String,
}

fn page_request(query: &ListOrdersQuery) -> PageRequest {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);
    PageRequest { limit, offset }
}

fn parse_json_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|error| ApiError::Unprocessable(error.to_string()))
}

/// Lists orders as a Serenade JSON [`Response`].
pub async fn list_admin_orders_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    catalog: &CatalogRepository,
    query: &ListOrdersQuery,
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    match catalog.list_orders(page_request(query)).await {
        Ok(orders) => json_response(
            200,
            &OrderListResponse {
                items: orders.into_iter().map(OrderResponse::from).collect(),
            },
        ),
        Err(error) => api_error_json_response(&ApiError::from_persist(&error)),
    }
}

/// Updates order fulfillment status as a Serenade JSON [`Response`].
pub async fn patch_admin_order_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    catalog: &CatalogRepository,
    order_id: &str,
    body: &[u8],
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    if let Err(error) = ensure_request_param(order_id) {
        return api_error_json_response(&error);
    }
    let request = match parse_json_body::<PatchOrderStatusRequest>(body) {
        Ok(request) => request,
        Err(error) => return api_error_json_response(&error),
    };
    let state = match OrderState::parse(&request.status) {
        Ok(state) => state,
        Err(error) => return api_error_json_response(&ApiError::from_domain(&error)),
    };
    match catalog.update_order_state(order_id, state).await {
        Ok(order) => json_response(200, &OrderResponse::from(order)),
        Err(error) => api_error_json_response(&ApiError::from_persist(&error)),
    }
}

/// `GET /v1/{admin_api_prefix}/orders` `OpenAPI` path (Serenade front).
#[utoipa::path(
    get,
    path = "/v1/{admin_api_prefix}/orders",
    params(ListOrdersQuery),
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Order page", body = OrderListResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn list_admin_orders() {}

/// `PATCH /v1/{admin_api_prefix}/orders/{id}` `OpenAPI` path (Serenade front).
#[utoipa::path(
    patch,
    path = "/v1/{admin_api_prefix}/orders/{id}",
    params(("id" = String, Path, description = "Order id")),
    request_body = PatchOrderStatusRequest,
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Updated order", body = OrderResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody),
        (status = 404, description = "Order not found", body = ErrorBody),
        (status = 422, description = "Invalid status", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn patch_admin_order() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list_query_string() {
        let query = ListOrdersQuery::from_query_string(Some("limit=3&offset=1"));
        assert_eq!(query.limit, Some(3));
        assert_eq!(query.offset, Some(1));
        assert_eq!(ListOrdersQuery::from_query_string(None).limit, None);
        let noisy = ListOrdersQuery::from_query_string(Some("&=1&foo=x&limit=nope"));
        assert_eq!(noisy.limit, None);
    }

    #[test]
    fn openapi_stubs_are_callable() {
        list_admin_orders();
        patch_admin_order();
    }
}

#[cfg(all(test, feature = "persist-sqlx"))]
mod admin_orders_response_tests {
    use super::*;
    use rustashop_persist_sqlx::{SqlxCatalogRepository, migrate, seed_catalog};
    use sqlx::postgres::PgPoolOptions;

    const SCHEMA_LOCK: i64 = 874_521;

    async fn seeded_catalog() -> (SqlxCatalogRepository, sqlx::PgPool) {
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
    async fn covers_auth_and_validation_errors() {
        let auth = AdminAuthConfig::from_token("secret");
        let (catalog, _pool) = seeded_catalog().await;
        assert_eq!(
            list_admin_orders_response(&auth, None, &catalog, &ListOrdersQuery::default())
                .await
                .status(),
            401
        );
        assert_eq!(
            patch_admin_order_response(&auth, None, &catalog, "id", br#"{"status":"paid"}"#)
                .await
                .status(),
            401
        );
        assert_eq!(
            patch_admin_order_response(
                &auth,
                Some("secret"),
                &catalog,
                "a\0b",
                br#"{"status":"paid"}"#,
            )
            .await
            .status(),
            422
        );
        assert_eq!(
            patch_admin_order_response(&auth, Some("secret"), &catalog, "oid", b"{")
                .await
                .status(),
            422
        );
        assert_eq!(
            patch_admin_order_response(
                &auth,
                Some("secret"),
                &catalog,
                "11111111-1111-1111-1111-111111111111",
                br#"{"status":"nope"}"#,
            )
            .await
            .status(),
            422
        );
        assert_eq!(
            patch_admin_order_response(
                &auth,
                Some("secret"),
                &catalog,
                "11111111-1111-1111-1111-111111111111",
                br#"{"status":"paid"}"#,
            )
            .await
            .status(),
            404
        );
    }

    #[tokio::test]
    async fn covers_persist_errors_on_closed_pool() {
        let auth = AdminAuthConfig::from_token("secret");
        let (catalog, pool) = seeded_catalog().await;
        pool.close().await;
        assert_eq!(
            list_admin_orders_response(
                &auth,
                Some("secret"),
                &catalog,
                &ListOrdersQuery::default()
            )
            .await
            .status(),
            500
        );
        assert_eq!(
            patch_admin_order_response(
                &auth,
                Some("secret"),
                &catalog,
                "11111111-1111-1111-1111-111111111111",
                br#"{"status":"paid"}"#,
            )
            .await
            .status(),
            500
        );
    }
}
