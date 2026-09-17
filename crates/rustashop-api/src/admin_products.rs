//! Admin product list and enabled PATCH (JSON via Serenade front; utoipa path items for `OpenAPI`).

use rustashop_persist::CatalogRepository;
use serde::Deserialize;
#[allow(unused_imports)]
use serde_json::json;
use serenade_contracts::PageRequest;
use serenade_http::Response;
use utoipa::{IntoParams, ToSchema};

use crate::admin_auth::AdminAuthConfig;
use crate::catalog_cache::CatalogCache;
use crate::error::{ApiError, ErrorBody, api_error_json_response, json_response};
use crate::products::{ProductListResponse, ProductResponse};
use crate::request_param::ensure_request_param;

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

/// Body for `PATCH /v1/{admin}/products/{id}`.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({"enabled": false}))]
pub struct PatchAdminProductRequest {
    /// Whether the product is offered on the storefront.
    #[schema(example = false)]
    pub enabled: bool,
}

fn page_request(query: &ListAdminProductsQuery) -> PageRequest {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);
    PageRequest { limit, offset }
}

fn parse_json_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|error| ApiError::Unprocessable(error.to_string()))
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

/// Updates product `enabled` and invalidates the storefront catalog cache tag.
pub async fn patch_admin_product_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    catalog: &CatalogRepository,
    product_id: &str,
    body: &[u8],
    cache: Option<&CatalogCache>,
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    if let Err(error) = ensure_request_param(product_id) {
        return api_error_json_response(&error);
    }
    let request = match parse_json_body::<PatchAdminProductRequest>(body) {
        Ok(request) => request,
        Err(error) => return api_error_json_response(&error),
    };
    match catalog
        .set_product_enabled(product_id, request.enabled)
        .await
    {
        Ok(product) => {
            if let Some(cache) = cache {
                let _ = cache.invalidate_catalog();
            }
            json_response(200, &ProductResponse::from(product))
        }
        Err(error) => api_error_json_response(&ApiError::from_persist(&error)),
    }
}

/// `GET /v1/{admin_api_prefix}/products` `OpenAPI` path (Serenade front).
#[utoipa::path(
    get,
    path = "/v1/{admin_api_prefix}/products",
    tag = "admin-products",
    params(ListAdminProductsQuery),
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Product page", body = ProductListResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn list_admin_products() {}

/// `PATCH /v1/{admin_api_prefix}/products/{id}` `OpenAPI` path (Serenade front).
#[utoipa::path(
    patch,
    path = "/v1/{admin_api_prefix}/products/{id}",
    tag = "admin-products",
    params(("id" = String, Path, description = "Product id")),
    request_body = PatchAdminProductRequest,
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Updated product", body = ProductResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody),
        (status = 404, description = "Unknown id", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn patch_admin_product() {}

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
    fn openapi_stubs_are_callable() {
        list_admin_products();
        patch_admin_product();
    }
}

#[cfg(all(test, feature = "persist-sqlx"))]
mod admin_products_response_tests {
    use super::*;
    use rustashop_persist_sqlx::{SqlxCatalogRepository, migrate, seed_catalog};
    use sqlx::postgres::PgPoolOptions;

    // Shared with other rustashop-api lib tests that reset `public`.
    const SCHEMA_LOCK: i64 = 874_521;
    const HOODIE_ID: &str = "22222222-2222-2222-2222-222222222221";

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
    async fn covers_auth_and_persist_errors() {
        let auth = AdminAuthConfig::from_token("secret");
        let (catalog, pool) = seeded_catalog().await;

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

    #[tokio::test]
    async fn patch_invalidates_catalog_cache() {
        let auth = AdminAuthConfig::from_token("secret");
        let (catalog, _pool) = seeded_catalog().await;
        let cache = CatalogCache::with_ttl(None);
        let list_key = CatalogCache::list_key(20, 0);
        cache.put_list(&list_key, ProductListResponse { items: Vec::new() });
        assert!(cache.get_list(&list_key).is_some());

        assert_eq!(
            patch_admin_product_response(
                &auth,
                None,
                &catalog,
                HOODIE_ID,
                br#"{"enabled":false}"#,
                Some(&cache),
            )
            .await
            .status(),
            401
        );
        assert_eq!(
            patch_admin_product_response(
                &auth,
                Some("secret"),
                &catalog,
                HOODIE_ID,
                br#"{"enabled":false}"#,
                Some(&cache),
            )
            .await
            .status(),
            200
        );
        assert!(cache.get_list(&list_key).is_none());

        // Restore so other suites see the seed product enabled.
        let _ = patch_admin_product_response(
            &auth,
            Some("secret"),
            &catalog,
            HOODIE_ID,
            br#"{"enabled":true}"#,
            None,
        )
        .await;

        assert_eq!(
            patch_admin_product_response(
                &auth,
                Some("secret"),
                &catalog,
                "a\0b",
                br#"{"enabled":true}"#,
                None,
            )
            .await
            .status(),
            422
        );
        assert_eq!(
            patch_admin_product_response(&auth, Some("secret"), &catalog, HOODIE_ID, b"{", None)
                .await
                .status(),
            422
        );
        assert_eq!(
            patch_admin_product_response(
                &auth,
                Some("secret"),
                &catalog,
                "00000000-0000-0000-0000-000000000000",
                br#"{"enabled":true}"#,
                None,
            )
            .await
            .status(),
            404
        );
    }
}
