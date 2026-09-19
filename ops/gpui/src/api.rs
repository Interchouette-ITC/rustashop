//! Blocking Commerce API client for the ops GPUI host.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::money::Money;

/// Order fulfillment statuses accepted by admin PATCH.
pub const ORDER_STATUSES: &[&str] = &["placed", "paid", "shipped", "cancelled"];

/// Which pane the ops window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    /// Orders list and status PATCH.
    Orders,
    /// Catalog sync (products + variant stock from detail).
    Catalog,
}

/// Connection settings for the Actix admin API.
#[derive(Clone, Debug)]
pub struct Config {
    /// Actix base URL without trailing slash (e.g. `http://127.0.0.1:8080`).
    pub api_base: String,
    /// Admin path segment (`RUSTASHOP_ADMIN_API_PREFIX`, default `admin`).
    pub admin_prefix: String,
    /// Bearer token (`RUSTASHOP_ADMIN_API_TOKEN`).
    pub token: String,
}

impl Config {
    /// Builds `{base}/v1/{prefix}/{path}`.
    #[must_use]
    pub fn admin_url(&self, path: &str) -> String {
        format!(
            "{}/v1/{}/{}",
            self.api_base.trim_end_matches('/'),
            self.admin_prefix.trim_matches('/'),
            path.trim_start_matches('/')
        )
    }

    /// Builds `{base}/v1/{path}` (public catalog detail for stock).
    #[must_use]
    pub fn public_url(&self, path: &str) -> String {
        format!(
            "{}/v1/{}",
            self.api_base.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }
}

/// Order row from `GET /v1/{admin}/orders`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Order {
    /// Order id.
    pub id: String,
    /// Human-readable number.
    pub number: String,
    /// Fulfillment state.
    pub state: String,
    /// Payment status.
    pub payment_status: String,
    /// Order currency.
    pub currency: String,
    /// Line sum.
    pub items_total: Money,
    /// Payable total.
    pub total: Money,
}

#[derive(Debug, Deserialize)]
struct OrderListResponse {
    items: Vec<Order>,
}

#[derive(Serialize)]
struct PatchOrderBody<'a> {
    status: &'a str,
}

/// Product row from admin list (no variants).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Product {
    /// Product id.
    pub id: String,
    /// URL slug.
    pub slug: String,
    /// Display name.
    pub name: String,
    /// Whether offered on the storefront.
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
struct ProductListResponse {
    items: Vec<Product>,
}

/// Variant from product detail (includes stock).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ProductVariant {
    /// Variant id.
    pub id: String,
    /// SKU.
    pub sku: String,
    /// Available units.
    pub stock_quantity: i32,
}

/// Product detail including variants (public `GET /v1/products/{id}`).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ProductDetail {
    /// Product id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Whether enabled.
    pub enabled: bool,
    /// Purchasable SKUs.
    #[serde(default)]
    pub variants: Vec<ProductVariant>,
}

/// Blocking HTTP client for ops screens.
pub struct AdminClient {
    http: reqwest::blocking::Client,
    config: Config,
}

impl AdminClient {
    /// Builds a client with a short request timeout.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn new(config: Config) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { http, config })
    }

    /// Returns the connection config.
    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
    }

    /// Lists admin orders (`limit=50`).
    ///
    /// # Errors
    ///
    /// Returns an error when the request or JSON decode fails.
    pub fn list_orders(&self) -> Result<Vec<Order>, String> {
        let url = self.config.admin_url("orders?limit=50");
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.config.token)
            .send()
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("orders list HTTP {}", resp.status()));
        }
        let body: OrderListResponse = resp.json().map_err(|e| e.to_string())?;
        Ok(body.items)
    }

    /// Patches order fulfillment status.
    ///
    /// # Errors
    ///
    /// Returns an error when the request or JSON decode fails.
    pub fn patch_order_status(&self, order_id: &str, status: &str) -> Result<Order, String> {
        let url = self.config.admin_url(&format!("orders/{order_id}"));
        let resp = self
            .http
            .patch(&url)
            .bearer_auth(&self.config.token)
            .header("Content-Type", "application/json")
            .json(&PatchOrderBody { status })
            .send()
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("order patch HTTP {}", resp.status()));
        }
        resp.json().map_err(|e| e.to_string())
    }

    /// Lists admin products including disabled (`limit=100`).
    ///
    /// # Errors
    ///
    /// Returns an error when the request or JSON decode fails.
    pub fn list_products(&self) -> Result<Vec<Product>, String> {
        let url = self.config.admin_url("products?limit=100");
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.config.token)
            .send()
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("products list HTTP {}", resp.status()));
        }
        let body: ProductListResponse = resp.json().map_err(|e| e.to_string())?;
        Ok(body.items)
    }

    /// Loads public product detail (variants + stock).
    ///
    /// # Errors
    ///
    /// Returns an error when the request or JSON decode fails.
    pub fn get_product_detail(&self, product_id: &str) -> Result<ProductDetail, String> {
        let url = self.config.public_url(&format!("products/{product_id}"));
        let resp = self.http.get(&url).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("product detail HTTP {}", resp.status()));
        }
        resp.json().map_err(|e| e.to_string())
    }

    /// Syncs catalog: admin product list plus detail stock for each row (online-first).
    ///
    /// # Errors
    ///
    /// Returns an error when listing products fails. Per-product detail failures are
    /// returned as empty-variant stubs with the product name preserved when possible.
    pub fn sync_catalog(&self) -> Result<Vec<ProductDetail>, String> {
        let products = self.list_products()?;
        let mut out = Vec::with_capacity(products.len());
        for product in products {
            match self.get_product_detail(&product.id) {
                Ok(detail) => out.push(detail),
                Err(_) => out.push(ProductDetail {
                    id: product.id,
                    name: product.name,
                    enabled: product.enabled,
                    variants: Vec::new(),
                }),
            }
        }
        Ok(out)
    }
}

/// Returns the next fulfillment status in the happy path, or `None` at terminal states.
#[must_use]
pub fn next_order_status(current: &str) -> Option<&'static str> {
    match current {
        "placed" => Some("paid"),
        "paid" => Some("shipped"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_url_joins_prefix() {
        let cfg = Config {
            api_base: "http://127.0.0.1:8080".into(),
            admin_prefix: "admin".into(),
            token: "t".into(),
        };
        assert_eq!(
            cfg.admin_url("orders"),
            "http://127.0.0.1:8080/v1/admin/orders"
        );
        assert_eq!(
            cfg.public_url("products/abc"),
            "http://127.0.0.1:8080/v1/products/abc"
        );
    }

    #[test]
    fn next_status_happy_path() {
        assert_eq!(next_order_status("placed"), Some("paid"));
        assert_eq!(next_order_status("paid"), Some("shipped"));
        assert_eq!(next_order_status("shipped"), None);
        assert_eq!(next_order_status("cancelled"), None);
    }

    #[test]
    fn order_statuses_match_admin() {
        assert_eq!(ORDER_STATUSES, &["placed", "paid", "shipped", "cancelled"]);
    }
}
