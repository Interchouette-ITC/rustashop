//! Public Commerce API catalog client for POS (online sync).

use std::time::Duration;

use serde::Deserialize;

use crate::money::Money;

/// Connection settings for the Actix public catalog.
#[derive(Clone, Debug)]
pub struct Config {
    /// Actix base URL without trailing slash.
    pub api_base: String,
}

impl Config {
    /// Builds `{base}/v1/{path}`.
    #[must_use]
    pub fn public_url(&self, path: &str) -> String {
        format!(
            "{}/v1/{}",
            self.api_base.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct ProductSummary {
    id: String,
    name: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct ProductListResponse {
    items: Vec<ProductSummary>,
}

/// Variant with price and stock from product detail.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ProductVariant {
    /// Variant id.
    pub id: String,
    /// SKU.
    pub sku: String,
    /// Optional label.
    pub name: Option<String>,
    /// Unit price.
    pub price: Money,
    /// Available units.
    pub stock_quantity: i32,
}

/// Product detail including variants.
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

/// Flattened sellable SKU for the sale pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SellableSku {
    /// Product id.
    pub product_id: String,
    /// Product name.
    pub product_name: String,
    /// Variant id.
    pub variant_id: String,
    /// SKU code.
    pub sku: String,
    /// Unit price.
    pub unit_price: Money,
    /// Available stock.
    pub stock_quantity: i32,
}

/// Blocking HTTP client for public catalog sync.
pub struct CatalogClient {
    http: reqwest::blocking::Client,
    config: Config,
}

impl CatalogClient {
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

    /// Lists enabled products (`limit=100`).
    ///
    /// # Errors
    ///
    /// Returns an error when the request or JSON decode fails.
    pub fn list_products(&self) -> Result<Vec<ProductDetail>, String> {
        let url = self.config.public_url("products?limit=100");
        let resp = self.http.get(&url).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("products list HTTP {}", resp.status()));
        }
        let body: ProductListResponse = resp.json().map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(body.items.len());
        for summary in body.items {
            match self.get_product_detail(&summary.id) {
                Ok(detail) => out.push(detail),
                Err(_) => out.push(ProductDetail {
                    id: summary.id,
                    name: summary.name,
                    enabled: summary.enabled,
                    variants: Vec::new(),
                }),
            }
        }
        Ok(out)
    }

    /// Loads one product detail.
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

    /// Syncs catalog into flattened sellable SKUs (enabled products with stock > 0).
    ///
    /// # Errors
    ///
    /// Returns an error when listing products fails.
    pub fn sync_sellable(&self) -> Result<Vec<SellableSku>, String> {
        let products = self.list_products()?;
        let mut skus = Vec::new();
        for product in products {
            if !product.enabled {
                continue;
            }
            for variant in product.variants {
                if variant.stock_quantity <= 0 {
                    continue;
                }
                skus.push(SellableSku {
                    product_id: product.id.clone(),
                    product_name: product.name.clone(),
                    variant_id: variant.id,
                    sku: variant.sku,
                    unit_price: variant.price,
                    stock_quantity: variant.stock_quantity,
                });
            }
        }
        Ok(skus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_url_joins() {
        let cfg = Config {
            api_base: "http://127.0.0.1:8080".into(),
        };
        assert_eq!(
            cfg.public_url("products"),
            "http://127.0.0.1:8080/v1/products"
        );
    }
}
