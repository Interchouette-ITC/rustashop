//! Catalog product handlers (JSON via Serenade front; `OpenAPI` stubs stay here).

use rustashop_domain::{Product, ProductVariant};
use rustashop_persist::CatalogRepository;
use serde::{Deserialize, Serialize};
use serenade_contracts::{PageRequest, ProductRepository};
use serenade_http::Response;
use utoipa::{IntoParams, ToSchema};

use crate::carts::MoneyResponse;
use crate::error::{api_error_json_response, json_response, ApiError, ErrorBody};
use crate::request_param::ensure_request_param;

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 100;

/// Query string for `GET /v1/products`.
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ListProductsQuery {
    /// Maximum rows (capped).
    pub limit: Option<u32>,
    /// Rows to skip.
    pub offset: Option<u32>,
}

impl ListProductsQuery {
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

/// Product JSON returned by list routes (no variants).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ProductResponse {
    /// Stable identifier.
    pub id: String,
    /// Optional category id.
    pub category_id: Option<String>,
    /// Unique URL slug.
    pub slug: String,
    /// Display name.
    pub name: String,
    /// Optional long description.
    pub description: Option<String>,
    /// Whether the product is offered for sale.
    pub enabled: bool,
}

impl From<Product> for ProductResponse {
    fn from(product: Product) -> Self {
        Self {
            id: product.id,
            category_id: product.category_id,
            slug: product.slug,
            name: product.name,
            description: product.description,
            enabled: product.enabled,
        }
    }
}

/// Variant JSON nested under product detail.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ProductVariantResponse {
    /// Stable identifier.
    pub id: String,
    /// Parent product id.
    pub product_id: String,
    /// Unique stock-keeping unit.
    pub sku: String,
    /// Optional variant label.
    pub name: Option<String>,
    /// Unit price.
    pub price: MoneyResponse,
    /// Available stock quantity.
    pub stock_quantity: i32,
}

impl From<ProductVariant> for ProductVariantResponse {
    fn from(variant: ProductVariant) -> Self {
        Self {
            id: variant.id,
            product_id: variant.product_id,
            sku: variant.sku,
            name: variant.name,
            price: MoneyResponse {
                amount_minor: variant.price.amount_minor,
                currency: variant.price.currency.as_str().to_owned(),
            },
            stock_quantity: variant.stock_quantity,
        }
    }
}

/// Product detail including purchasable variants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ProductDetailResponse {
    /// Stable identifier.
    pub id: String,
    /// Optional category id.
    pub category_id: Option<String>,
    /// Unique URL slug.
    pub slug: String,
    /// Display name.
    pub name: String,
    /// Optional long description.
    pub description: Option<String>,
    /// Whether the product is offered for sale.
    pub enabled: bool,
    /// Purchasable SKUs for this product.
    pub variants: Vec<ProductVariantResponse>,
}

impl ProductDetailResponse {
    fn from_parts(product: Product, variants: Vec<ProductVariant>) -> Self {
        Self {
            id: product.id,
            category_id: product.category_id,
            slug: product.slug,
            name: product.name,
            description: product.description,
            enabled: product.enabled,
            variants: variants
                .into_iter()
                .map(ProductVariantResponse::from)
                .collect(),
        }
    }
}

/// List payload for `GET /v1/products`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ProductListResponse {
    /// Page of products.
    pub items: Vec<ProductResponse>,
}

fn page_request(query: &ListProductsQuery) -> PageRequest {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);
    PageRequest { limit, offset }
}

/// Lists enabled products as a Serenade JSON [`Response`].
pub async fn list_products_response(
    catalog: &CatalogRepository,
    query: &ListProductsQuery,
) -> Response {
    match ProductRepository::list(catalog, page_request(query)).await {
        Ok(items) => json_response(
            200,
            &ProductListResponse {
                items: items.into_iter().map(ProductResponse::from).collect(),
            },
        ),
        Err(error) => api_error_json_response(&ApiError::from_persist(&error)),
    }
}

/// Returns one product by id (with variants) as a Serenade JSON [`Response`].
pub async fn get_product_response(catalog: &CatalogRepository, id: &str) -> Response {
    if let Err(error) = ensure_request_param(id) {
        return api_error_json_response(&error);
    }
    let id = id.to_owned();
    let product = match ProductRepository::find_by_id(catalog, &id).await {
        Ok(product) => product,
        Err(error) => return api_error_json_response(&ApiError::from_persist(&error)),
    };
    let Some(product) = product else {
        return api_error_json_response(&ApiError::NotFound);
    };
    match catalog.list_variants_for_product(&id).await {
        Ok(variants) => json_response(200, &ProductDetailResponse::from_parts(product, variants)),
        Err(error) => api_error_json_response(&ApiError::from_persist(&error)),
    }
}

/// `GET /v1/products` `OpenAPI` path (served by the Serenade HTTP front controller).
#[utoipa::path(
    get,
    path = "/v1/products",
    params(ListProductsQuery),
    responses((status = 200, description = "Product page", body = ProductListResponse))
)]
#[allow(clippy::missing_const_for_fn)]
pub fn list_products() {}

/// `GET /v1/products/{id}` `OpenAPI` path (served by the Serenade HTTP front controller).
#[utoipa::path(
    get,
    path = "/v1/products/{id}",
    params(("id" = String, Path, description = "Product id")),
    responses(
        (status = 200, description = "Product with variants", body = ProductDetailResponse),
        (status = 404, description = "Unknown id", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn get_product() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list_query_string() {
        let query = ListProductsQuery::from_query_string(Some("limit=5&offset=10"));
        assert_eq!(query.limit, Some(5));
        assert_eq!(query.offset, Some(10));
        assert_eq!(ListProductsQuery::from_query_string(None).limit, None);
        assert_eq!(ListProductsQuery::from_query_string(Some("")).limit, None);
        let noisy = ListProductsQuery::from_query_string(Some("&=1&foo=bar&limit=nope"));
        assert_eq!(noisy.limit, None);
        assert_eq!(noisy.offset, None);
    }

    #[test]
    fn openapi_stubs_are_callable() {
        list_products();
        get_product();
    }
}

#[cfg(all(test, feature = "persist-sqlx"))]
mod catalog_error_tests {
    use super::*;
    use rustashop_persist_sqlx::SqlxCatalogRepository;
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    async fn maps_nul_and_closed_pool_errors() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skip: DATABASE_URL is not set");
            return;
        };
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        let catalog = SqlxCatalogRepository::new(pool.clone());
        let nul = get_product_response(&catalog, "a\0b").await;
        assert_eq!(nul.status(), 422);
        pool.close().await;
        let list = list_products_response(&catalog, &ListProductsQuery::default()).await;
        assert_eq!(list.status(), 500);
        let get = get_product_response(&catalog, "22222222-2222-2222-2222-222222222221").await;
        assert_eq!(get.status(), 500);
    }
}
