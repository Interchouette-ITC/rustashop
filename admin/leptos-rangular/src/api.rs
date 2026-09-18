//! Admin Commerce API client (`/api` → Actix; Trunk proxy).

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

use crate::money::Money;

const API_BASE: &str = "/api";
/// Must match `RUSTASHOP_ADMIN_API_PREFIX` (Angular `environment.adminApiPrefix`).
const ADMIN_PREFIX: &str = "admin";

fn admin_url(path: &str) -> String {
    format!("{API_BASE}/v1/{ADMIN_PREFIX}/{path}")
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
        assert_eq!(admin_url("orders"), "/api/v1/admin/orders");
        assert_eq!(admin_url("orders/abc"), "/api/v1/admin/orders/abc");
    }

    #[test]
    fn order_statuses_match_angular() {
        assert_eq!(ORDER_STATUSES, &["placed", "paid", "shipped", "cancelled"]);
    }
}
