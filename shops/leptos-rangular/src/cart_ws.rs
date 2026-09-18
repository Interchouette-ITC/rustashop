//! Cart WebSocket URL + `cart.updated` parse (same contract as Angular N1).

use serde::Deserialize;

use crate::api::Cart;

const API_BASE: &str = "/api";

#[derive(Debug, Deserialize)]
struct CartUpdatedEnvelope {
    #[serde(rename = "type")]
    event_type: String,
    cart: Cart,
}

/// Parses a cart socket text frame.
///
/// Returns the cart snapshot for `cart.updated`, otherwise `None`.
#[must_use]
pub fn parse_cart_updated_message(raw: &str) -> Option<Cart> {
    let envelope: CartUpdatedEnvelope = serde_json::from_str(raw).ok()?;
    if envelope.event_type != "cart.updated" {
        return None;
    }
    if envelope.cart.id.is_empty() || envelope.cart.token.is_empty() {
        return None;
    }
    Some(envelope.cart)
}

/// Builds `ws(s)://…/v1/carts/{id}/ws?token=` from an HTTP API base URL.
///
/// Relative bases (e.g. `/api`) resolve against `window.location.origin` on wasm.
///
/// # Errors
///
/// Returns an error when the browser origin cannot be read (wasm) or the URL is invalid.
pub fn cart_ws_url(api_base: &str, cart_id: &str, token: &str) -> Result<String, String> {
    let base = api_base.trim_end_matches('/');
    let http_base = if base.starts_with("http://") || base.starts_with("https://") {
        base.to_owned()
    } else {
        let origin = browser_origin()?;
        let path = if base.starts_with('/') {
            base.to_owned()
        } else {
            format!("/{base}")
        };
        format!("{origin}{path}")
    };
    let ws_base = http_base.replacen("http", "ws", 1);
    let id = urlencoding_encode(cart_id);
    let tok = urlencoding_encode(token);
    Ok(format!("{ws_base}/v1/carts/{id}/ws?token={tok}"))
}

/// Default shop API base (`/api`, same as Angular).
#[must_use]
pub const fn default_api_base() -> &'static str {
    API_BASE
}

fn browser_origin() -> Result<String, String> {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .ok_or_else(|| "no window".to_owned())?
            .location()
            .origin()
            .map_err(|_| "location.origin unavailable".to_owned())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Err("relative API base needs an absolute URL in native tests".to_owned())
    }
}

/// Minimal encode for path/query tokens (id and cart token are opaque ASCII).
fn urlencoding_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{cart_ws_url, parse_cart_updated_message};

    #[test]
    fn parse_cart_updated_returns_snapshot() {
        let raw = r#"{
            "type":"cart.updated",
            "version":1,
            "cart":{
                "id":"cart-1",
                "token":"tok",
                "status":"open",
                "currency":"EUR",
                "lines":[],
                "items_total":{"amount_minor":0,"currency":"EUR"}
            }
        }"#;
        let cart = parse_cart_updated_message(raw).expect("cart");
        assert_eq!(cart.id, "cart-1");
        assert_eq!(cart.token, "tok");
    }

    #[test]
    fn parse_ignores_unknown_type() {
        assert!(parse_cart_updated_message(r#"{"type":"job.log"}"#).is_none());
    }

    #[test]
    fn parse_ignores_invalid_json() {
        assert!(parse_cart_updated_message("not-json").is_none());
    }

    #[test]
    fn absolute_http_base_maps_to_ws() {
        let url = cart_ws_url("http://127.0.0.1:8080", "c1", "t1").expect("url");
        assert_eq!(url, "ws://127.0.0.1:8080/v1/carts/c1/ws?token=t1");
    }

    #[test]
    fn encodes_token_special_chars() {
        let url = cart_ws_url("http://127.0.0.1:8080", "c2", "tok space").expect("url");
        assert!(url.contains("token=tok%20space"));
    }
}
