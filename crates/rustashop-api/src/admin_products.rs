//! Admin product list (JSON via Serenade front; utoipa path items for `OpenAPI`).

use rustashop_persist::CatalogRepository;
use serde::Deserialize;
use serenade_contracts::PageRequest;
use serenade_http::Response;
use utoipa::IntoParams;

use crate::admin_auth::AdminAuthConfig;
use crate::error::{api_error_json_response, json_response, ApiError, ErrorBody};
use crate::products::{ProductListResponse, ProductResponse};

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 100;

/// Query string for admin product list.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListAdminProductsQuery {
    /// Maximum rows (capped).
    pub limit: Option<u32>,
    /// Rows to skip.
    pub offset: Option<u32>,
}

impl ListAdminProductsQuery {
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

fn page_request(query: &ListAdminProductsQuery) -> PageRequest {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);
    PageRequest { limit, offset }
}

/// Lists all products (including disabled) as a Serenade JSON [`Response`].
pub async fn list_admin_products_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    catalog: &CatalogRepository,
    query: &ListAdminProductsQuery,
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    match catalog.list_all_products(page_request(query)).await {
        Ok(products) => json_response(
            200,
            &ProductListResponse {
                items: products.into_iter().map(ProductResponse::from).collect(),
            },
        ),
        Err(error) => api_error_json_response(&ApiError::from_persist(&error)),
    }
}

/// `GET /v1/{admin_api_prefix}/products` `OpenAPI` path (Serenade front).
#[utoipa::path(
    get,
    path = "/v1/{admin_api_prefix}/products",
    params(ListAdminProductsQuery),
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Product page", body = ProductListResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn list_admin_products() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list_query_string() {
        let query = ListAdminProductsQuery::from_query_string(Some("limit=5&offset=2"));
        assert_eq!(query.limit, Some(5));
        assert_eq!(query.offset, Some(2));
        assert_eq!(ListAdminProductsQuery::from_query_string(None).limit, None);
        let noisy = ListAdminProductsQuery::from_query_string(Some("&=1&foo=bar&limit=nope"));
        assert_eq!(noisy.limit, None);
        assert_eq!(noisy.offset, None);
    }

    #[test]
    fn openapi_stub_is_callable() {
        list_admin_products();
    }
}

#[cfg(all(test, feature = "persist-sqlx"))]
mod admin_products_response_tests {
    use super::*;
    use rustashop_persist_sqlx::{migrate, seed_catalog, SqlxCatalogRepository};
    use sqlx::postgres::PgPoolOptions;

    // Shared with other rustashop-api lib tests that reset `public`.
    const SCHEMA_LOCK: i64 = 874_521;

    #[tokio::test]
    async fn covers_auth_and_persist_errors() {
        let auth = AdminAuthConfig::from_token("secret");
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
        let catalog = SqlxCatalogRepository::new(pool.clone());

        assert_eq!(
            list_admin_products_response(&auth, None, &catalog, &ListAdminProductsQuery::default())
                .await
                .status(),
            401
        );
        assert_eq!(
            list_admin_products_response(
                &auth,
                Some("secret"),
                &catalog,
                &ListAdminProductsQuery::default(),
            )
            .await
            .status(),
            200
        );

        pool.close().await;
        assert_eq!(
            list_admin_products_response(
                &auth,
                Some("secret"),
                &catalog,
                &ListAdminProductsQuery::default(),
            )
            .await
            .status(),
            500
        );
    }
}
