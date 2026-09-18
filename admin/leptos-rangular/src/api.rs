//! Admin Commerce API client (Trunk `/api` proxy or absolute Actix base).

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

use crate::money::Money;

/// sessionStorage override for desktop / tip hosts (no trailing slash).
pub const API_BASE_STORAGE_KEY: &str = "rs.adminApiBase";

/// Browser / Trunk default (proxied to Actix).
pub const API_BASE_BROWSER: &str = "/api";
/// Desktop / non-http origin default (Actix on loopback).
pub const API_BASE_DESKTOP: &str = "http://127.0.0.1:8080";
/// Must match `RUSTASHOP_ADMIN_API_PREFIX` (Angular `environment.adminApiPrefix`).
const ADMIN_PREFIX: &str = "admin";

fn admin_url(path: &str) -> String {
    admin_url_with_base(&api_base(), path)
}

fn admin_url_with_base(base: &str, path: &str) -> String {
    format!("{}/v1/{ADMIN_PREFIX}/{path}", base.trim_end_matches('/'))
}

/// Resolves the Commerce API base: storage override, else `/api` on http(s), else loopback Actix.
#[must_use]
pub fn api_base() -> String {
    let (browser, desktop) = default_api_bases();
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(stored) = read_stored_api_base() {
            return stored;
        }
        if browser_http_origin() {
            return browser.to_owned();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = browser;
    desktop.to_owned()
}

/// Browser and desktop default bases (for docs / UI hints).
#[must_use]
pub const fn default_api_bases() -> (&'static str, &'static str) {
    (API_BASE_BROWSER, API_BASE_DESKTOP)
}

/// Persists an API base override (trimmed; empty clears).
pub fn save_api_base(raw: &str) {
    let trimmed = raw.trim().trim_end_matches('/').to_owned();
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = window_session_storage() {
        if trimmed.is_empty() {
            let _ = storage.remove_item(API_BASE_STORAGE_KEY);
        } else {
            let _ = storage.set_item(API_BASE_STORAGE_KEY, &trimmed);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = trimmed;
}

#[cfg(target_arch = "wasm32")]
fn read_stored_api_base() -> Option<String> {
    window_session_storage()
        .and_then(|s| s.get_item(API_BASE_STORAGE_KEY).ok().flatten())
        .map(|s| s.trim().trim_end_matches('/').to_owned())
        .filter(|s| !s.is_empty())
}

#[cfg(target_arch = "wasm32")]
fn browser_http_origin() -> bool {
    web_sys::window()
        .and_then(|w| w.location().protocol().ok())
        .is_some_and(|p| p == "http:" || p == "https:")
}

#[cfg(target_arch = "wasm32")]
fn window_session_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok().flatten()
}

/// Order fulfillment statuses accepted by admin PATCH.
pub const ORDER_STATUSES: &[&str] = &["placed", "paid", "shipped", "cancelled"];

/// Order line from admin list/detail payloads.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct OrderLine {
    pub id: String,
    pub quantity: i32,
    pub unit_price: Money,
    pub line_total: Money,
}

/// Order row from `GET /v1/{admin}/orders`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Order {
    pub id: String,
    pub number: String,
    pub state: String,
    pub payment_status: String,
    pub currency: String,
    pub items_total: Money,
    pub total: Money,
    #[serde(default)]
    pub lines: Vec<OrderLine>,
}

/// Paginated order list.
#[derive(Clone, Debug, Deserialize)]
pub struct OrderListResponse {
    pub items: Vec<Order>,
}

#[derive(Serialize)]
struct PatchOrderBody<'a> {
    status: &'a str,
}

/// Product row from `GET /v1/{admin}/products`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Product {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub enabled: bool,
}

/// Paginated product list.
#[derive(Clone, Debug, Deserialize)]
pub struct ProductListResponse {
    pub items: Vec<Product>,
}

/// Lists admin orders with bearer auth.
///
/// # Errors
///
/// Returns a human-readable error when the request or JSON decode fails.
pub async fn list_orders(token: &str) -> Result<Vec<Order>, String> {
    let resp = Request::get(&admin_url("orders?limit=50"))
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("orders list HTTP {}", resp.status()));
    }
    let body: OrderListResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body.items)
}

/// Updates order fulfillment status via admin `PATCH`.
///
/// # Errors
///
/// Returns a human-readable error when the request or JSON decode fails.
pub async fn patch_order_status(
    token: &str,
    order_id: &str,
    status: &str,
) -> Result<Order, String> {
    let resp = Request::patch(&admin_url(&format!("orders/{order_id}")))
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&PatchOrderBody { status })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("order patch HTTP {}", resp.status()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

/// Lists admin products (includes disabled).
///
/// # Errors
///
/// Returns a human-readable error when the request or JSON decode fails.
pub async fn list_products(token: &str) -> Result<Vec<Product>, String> {
    let resp = Request::get(&admin_url("products?limit=100"))
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("products list HTTP {}", resp.status()));
    }
    let body: ProductListResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body.items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_url_uses_prefix() {
        assert_eq!(
            admin_url_with_base("/api", "orders"),
            "/api/v1/admin/orders"
        );
        assert_eq!(
            admin_url_with_base("http://127.0.0.1:8080", "orders/abc"),
            "http://127.0.0.1:8080/v1/admin/orders/abc"
        );
    }

    #[test]
    fn api_base_constants() {
        assert_eq!(default_api_bases(), ("/api", "http://127.0.0.1:8080"));
        assert_eq!(API_BASE_STORAGE_KEY, "rs.adminApiBase");
    }

    #[test]
    fn order_statuses_match_angular() {
        assert_eq!(ORDER_STATUSES, &["placed", "paid", "shipped", "cancelled"]);
    }

    #[test]
    fn order_deserializes_admin_payload() {
        let json = r#"{
            "id":"o1","number":"1001","state":"placed","payment_status":"pending",
            "currency":"EUR","items_total":{"amount_minor":1000,"currency":"EUR"},
            "total":{"amount_minor":1000,"currency":"EUR"},
            "lines":[{"id":"l1","quantity":1,
              "unit_price":{"amount_minor":1000,"currency":"EUR"},
              "line_total":{"amount_minor":1000,"currency":"EUR"}}]
        }"#;
        let order: Order = serde_json::from_str(json).unwrap();
        assert_eq!(order.number, "1001");
        assert_eq!(order.lines.len(), 1);
        assert_eq!(order.total.display(), "10.00 EUR");
    }

    #[test]
    fn product_deserializes_admin_payload() {
        let json = r#"{"id":"p1","slug":"mug","name":"Mug","enabled":true}"#;
        let product: Product = serde_json::from_str(json).unwrap();
        assert!(product.enabled);
        assert_eq!(product.slug, "mug");
    }
}
