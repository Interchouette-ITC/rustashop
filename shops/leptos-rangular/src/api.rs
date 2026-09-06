//! Commerce API client (same `/api` prefix as the Angular shop; Trunk proxies to Actix).

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

const API_BASE: &str = "/api";

/// Money amount from the Commerce API (`amount_minor` + ISO currency).
#[derive(Clone, Debug, Deserialize)]
pub struct Money {
    pub amount_minor: i64,
    pub currency: String,
}

impl Money {
    /// Formats as `major.cents CURRENCY` (minor units ÷ 100).
    #[must_use]
    pub fn display(&self) -> String {
        let major = self.amount_minor / 100;
        let cents = self.amount_minor.rem_euclid(100);
        format!("{major}.{cents:02} {}", self.currency)
    }
}

/// Catalog product row from `GET /v1/products`.
#[derive(Clone, Debug, Deserialize)]
pub struct Product {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
}

impl Product {
    /// Whether the product is listed in the shop catalog.
    #[must_use]
    pub const fn is_listed(&self) -> bool {
        self.enabled
    }
}

/// Paginated product list body.
#[derive(Clone, Debug, Deserialize)]
pub struct ProductListResponse {
    pub items: Vec<Product>,
}

/// Sellable variant under a product detail.
#[derive(Clone, Debug, Deserialize)]
pub struct ProductVariant {
    pub id: String,
    pub product_id: String,
    pub sku: String,
    pub name: Option<String>,
    pub price: Money,
    pub stock_quantity: i32,
}

impl ProductVariant {
    /// Parent product id from the API payload.
    #[must_use]
    pub fn parent_product_id(&self) -> &str {
        &self.product_id
    }
}

/// Product detail with variants from `GET /v1/products/{id}`.
#[derive(Clone, Debug, Deserialize)]
pub struct ProductDetail {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub variants: Vec<ProductVariant>,
}

impl ProductDetail {
    /// Whether the product is listed; disabled products stay reachable by id for debugging.
    #[must_use]
    pub const fn is_listed(&self) -> bool {
        self.enabled
    }
}

/// One line in a cart snapshot.
#[derive(Clone, Debug, Deserialize)]
pub struct CartLine {
    pub id: String,
    pub variant_id: String,
    pub quantity: i32,
    pub unit_price: Money,
    pub line_total: Money,
    pub product_name: String,
    pub variant_sku: String,
}

impl CartLine {
    /// Variant id used when mutating the line on the API.
    #[must_use]
    pub fn variant_ref(&self) -> &str {
        &self.variant_id
    }
}

/// Cart snapshot from create/get/add-line responses.
#[derive(Clone, Debug, Deserialize)]
pub struct Cart {
    pub id: String,
    pub status: String,
    pub currency: String,
    pub lines: Vec<CartLine>,
    pub items_total: Money,
}

impl Cart {
    /// Cart currency code (matches `items_total.currency` when consistent).
    #[must_use]
    pub fn currency_code(&self) -> &str {
        &self.currency
    }
}

#[derive(Serialize)]
struct AddLineBody<'body> {
    variant_id: &'body str,
    quantity: i32,
}

fn url(path: &str) -> String {
    format!("{API_BASE}{path}")
}

async fn read_json<T: for<'de> Deserialize<'de>>(resp: gloo_net::http::Response) -> Result<T, String> {
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<T>()
        .await
        .map_err(|err| format!("decode: {err}"))
}

/// `GET /v1/products`.
pub async fn list_products() -> Result<ProductListResponse, String> {
    let resp = Request::get(&url("/v1/products"))
        .send()
        .await
        .map_err(|err| format!("network: {err}"))?;
    read_json(resp).await
}

/// `GET /v1/products/{id}`.
pub async fn get_product(id: &str) -> Result<ProductDetail, String> {
    let resp = Request::get(&url(&format!("/v1/products/{id}")))
        .send()
        .await
        .map_err(|err| format!("network: {err}"))?;
    read_json(resp).await
}

/// `POST /v1/carts` with an empty JSON body.
pub async fn create_cart() -> Result<Cart, String> {
    let resp = Request::post(&url("/v1/carts"))
        .header("content-type", "application/json")
        .body("{}")
        .map_err(|err| format!("body: {err}"))?
        .send()
        .await
        .map_err(|err| format!("network: {err}"))?;
    read_json(resp).await
}

/// `GET /v1/carts/{id}`.
pub async fn get_cart(id: &str) -> Result<Cart, String> {
    let resp = Request::get(&url(&format!("/v1/carts/{id}")))
        .send()
        .await
        .map_err(|err| format!("network: {err}"))?;
    read_json(resp).await
}

/// `POST /v1/carts/{id}/lines`.
pub async fn add_cart_line(cart_id: &str, variant_id: &str, quantity: i32) -> Result<Cart, String> {
    let body = serde_json::to_string(&AddLineBody {
        variant_id,
        quantity,
    })
    .map_err(|err| format!("encode: {err}"))?;
    let resp = Request::post(&url(&format!("/v1/carts/{cart_id}/lines")))
        .header("content-type", "application/json")
        .body(body)
        .map_err(|err| format!("body: {err}"))?
        .send()
        .await
        .map_err(|err| format!("network: {err}"))?;
    read_json(resp).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_display_formats_minor_units() {
        let money = Money {
            amount_minor: 1250,
            currency: "EUR".into(),
        };
        assert_eq!(money.display(), "12.50 EUR");
        let zero = Money {
            amount_minor: 5,
            currency: "USD".into(),
        };
        assert_eq!(zero.display(), "0.05 USD");
    }

    #[test]
    fn product_listing_flags() {
        let listed = Product {
            id: "1".into(),
            slug: "a".into(),
            name: "A".into(),
            description: None,
            enabled: true,
        };
        assert!(listed.is_listed());
        let detail = ProductDetail {
            id: "1".into(),
            slug: "a".into(),
            name: "A".into(),
            description: None,
            enabled: false,
            variants: vec![],
        };
        assert!(!detail.is_listed());
    }

    #[test]
    fn cart_and_line_refs() {
        let line = CartLine {
            id: "l1".into(),
            variant_id: "v1".into(),
            quantity: 2,
            unit_price: Money {
                amount_minor: 100,
                currency: "EUR".into(),
            },
            line_total: Money {
                amount_minor: 200,
                currency: "EUR".into(),
            },
            product_name: "P".into(),
            variant_sku: "SKU".into(),
        };
        assert_eq!(line.variant_ref(), "v1");
        let cart = Cart {
            id: "c1".into(),
            status: "open".into(),
            currency: "EUR".into(),
            lines: vec![line],
            items_total: Money {
                amount_minor: 200,
                currency: "EUR".into(),
            },
        };
        assert_eq!(cart.currency_code(), "EUR");
        let variant = ProductVariant {
            id: "v1".into(),
            product_id: "p1".into(),
            sku: "SKU".into(),
            name: None,
            price: Money {
                amount_minor: 100,
                currency: "EUR".into(),
            },
            stock_quantity: 3,
        };
        assert_eq!(variant.parent_product_id(), "p1");
    }
}
