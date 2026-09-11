//! HTTP client that maps MCP tools onto the commerce `OpenAPI` surface.

use serde_json::json;

use crate::tools::{
    AddCartLineInput, AdminListInput, CartLineRefInput, CreateCartInput, GetCartInput,
    GetProductInput, ListProductsInput, PatchOrderStatusInput, PlaceOrderInput, ToolEffect,
    tool_by_name,
};

/// Env: commerce API base URL (no trailing slash).
pub const API_BASE_ENV: &str = "RUSTASHOP_API_BASE";

/// Env: allow MCP to invoke commit-class tools (`place_order`, `patch_order_status`).
pub const ALLOW_COMMIT_ENV: &str = "RUSTASHOP_MCP_ALLOW_COMMIT";

/// Env: admin bearer for admin-scoped tools.
pub const ADMIN_TOKEN_ENV: &str = "RUSTASHOP_ADMIN_API_TOKEN";

/// Env: opaque admin URI segment (default `admin`).
pub const ADMIN_PREFIX_ENV: &str = "RUSTASHOP_ADMIN_API_PREFIX";

/// Proxies tool calls to `rustashop-api`.
#[derive(Clone)]
pub struct CommerceClient {
    base: String,
    http: reqwest::Client,
    admin_token: Option<String>,
    admin_prefix: String,
    allow_commit: bool,
}

impl CommerceClient {
    /// Loads base URL, admin auth, and commit gate from the environment.
    #[must_use]
    pub fn from_env() -> Self {
        let base = std::env::var(API_BASE_ENV).unwrap_or_else(|_| "http://127.0.0.1:8080".into());
        let allow_commit = matches!(
            std::env::var(ALLOW_COMMIT_ENV).as_deref(),
            Ok("1" | "true" | "TRUE" | "yes" | "YES")
        );
        let admin_token = std::env::var(ADMIN_TOKEN_ENV)
            .ok()
            .filter(|s| !s.is_empty());
        let admin_prefix = std::env::var(ADMIN_PREFIX_ENV).unwrap_or_else(|_| "admin".into());
        Self {
            base: base.trim_end_matches('/').to_owned(),
            http: reqwest::Client::new(),
            admin_token,
            admin_prefix,
            allow_commit,
        }
    }

    /// Test helper with fixed settings (no network until a method runs).
    #[must_use]
    pub fn new_for_test(base: impl Into<String>, allow_commit: bool) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            http: reqwest::Client::new(),
            admin_token: None,
            admin_prefix: "admin".into(),
            allow_commit,
        }
    }

    /// Sets admin bearer + prefix for tests.
    #[must_use]
    pub fn with_admin(mut self, token: impl Into<String>, prefix: impl Into<String>) -> Self {
        self.admin_token = Some(token.into());
        self.admin_prefix = prefix.into();
        self
    }

    /// Whether commit-class tools are enabled.
    #[must_use]
    pub const fn allow_commit(&self) -> bool {
        self.allow_commit
    }

    /// Refuses commit tools when the allow-commit env gate is off.
    ///
    /// # Errors
    ///
    /// Returns an explanatory string when the tool is commit-class and gated.
    pub fn refuse_commit_if_gated(&self, tool_name: &str) -> Result<(), String> {
        let Some(desc) = tool_by_name(tool_name) else {
            return Ok(());
        };
        if desc.effect == ToolEffect::Commit && !self.allow_commit {
            return Err(format!(
                "tool `{tool_name}` is commit-class; set {ALLOW_COMMIT_ENV}=1 to allow MCP commits"
            ));
        }
        Ok(())
    }

    async fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
        let url = format!("{}{path}", self.base);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|err| err.to_string())?;
        Self::json_body(resp).await
    }

    async fn get_json_admin(&self, path: &str) -> Result<serde_json::Value, String> {
        let url = format!("{}{path}", self.base);
        let mut req = self.http.get(&url);
        if let Some(token) = &self.admin_token {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await.map_err(|err| err.to_string())?;
        Self::json_body(resp).await
    }

    async fn send_json(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
        admin: bool,
        idempotency: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let url = format!("{}{path}", self.base);
        let mut req = self.http.request(method, &url);
        if admin && let Some(token) = &self.admin_token {
            req = req.bearer_auth(token);
        }
        if let Some(key) = idempotency {
            req = req.header("Idempotency-Key", key);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req.send().await.map_err(|err| err.to_string())?;
        Self::json_body(resp).await
    }

    async fn json_body(resp: reqwest::Response) -> Result<serde_json::Value, String> {
        let status = resp.status();
        let text = resp.text().await.map_err(|err| err.to_string())?;
        if !status.is_success() {
            return Err(format!("HTTP {status}: {text}"));
        }
        if text.is_empty() {
            return Ok(json!({ "ok": true }));
        }
        serde_json::from_str(&text).map_err(|err| format!("invalid JSON: {err}; body={text}"))
    }

    /// `list_products`.
    ///
    /// # Errors
    ///
    /// Returns transport, HTTP status, or JSON parse failures as a string.
    pub async fn list_products(
        &self,
        input: &ListProductsInput,
    ) -> Result<serde_json::Value, String> {
        let mut path = "/v1/products".to_owned();
        let mut q = Vec::new();
        if let Some(limit) = input.limit {
            q.push(format!("limit={limit}"));
        }
        if let Some(offset) = input.offset {
            q.push(format!("offset={offset}"));
        }
        if !q.is_empty() {
            path.push('?');
            path.push_str(&q.join("&"));
        }
        self.get_json(&path).await
    }

    /// `get_product`.
    ///
    /// # Errors
    ///
    /// Returns transport, HTTP status, or JSON parse failures as a string.
    pub async fn get_product(&self, input: &GetProductInput) -> Result<serde_json::Value, String> {
        self.get_json(&format!("/v1/products/{}", input.id)).await
    }

    /// `create_cart`.
    ///
    /// # Errors
    ///
    /// Returns transport, HTTP status, or JSON parse failures as a string.
    pub async fn create_cart(&self, input: &CreateCartInput) -> Result<serde_json::Value, String> {
        let body = serde_json::to_value(input).map_err(|err| err.to_string())?;
        self.send_json(reqwest::Method::POST, "/v1/carts", Some(&body), false, None)
            .await
    }

    /// `get_cart`.
    ///
    /// # Errors
    ///
    /// Returns transport, HTTP status, or JSON parse failures as a string.
    pub async fn get_cart(&self, input: &GetCartInput) -> Result<serde_json::Value, String> {
        self.get_json(&format!("/v1/carts/{}", input.id)).await
    }

    /// `add_cart_line`.
    ///
    /// # Errors
    ///
    /// Returns transport, HTTP status, or JSON parse failures as a string.
    pub async fn add_cart_line(
        &self,
        input: &AddCartLineInput,
    ) -> Result<serde_json::Value, String> {
        let body = json!({
            "variant_id": input.variant_id,
            "quantity": input.quantity,
        });
        self.send_json(
            reqwest::Method::POST,
            &format!("/v1/carts/{}/lines", input.cart_id),
            Some(&body),
            false,
            None,
        )
        .await
    }

    /// `update_cart_line`.
    ///
    /// # Errors
    ///
    /// Returns a missing-quantity error, or transport / HTTP / JSON failures.
    pub async fn update_cart_line(
        &self,
        input: &CartLineRefInput,
    ) -> Result<serde_json::Value, String> {
        let qty = input
            .quantity
            .ok_or_else(|| "update_cart_line requires quantity".to_owned())?;
        let body = json!({ "quantity": qty });
        self.send_json(
            reqwest::Method::PATCH,
            &format!("/v1/carts/{}/lines/{}", input.cart_id, input.line_id),
            Some(&body),
            false,
            None,
        )
        .await
    }

    /// `delete_cart_line`.
    ///
    /// # Errors
    ///
    /// Returns transport, HTTP status, or JSON parse failures as a string.
    pub async fn delete_cart_line(
        &self,
        input: &CartLineRefInput,
    ) -> Result<serde_json::Value, String> {
        self.send_json(
            reqwest::Method::DELETE,
            &format!("/v1/carts/{}/lines/{}", input.cart_id, input.line_id),
            None,
            false,
            None,
        )
        .await
    }

    /// `place_order`.
    ///
    /// # Errors
    ///
    /// Returns a commit-gate refusal, or transport / HTTP / JSON failures.
    pub async fn place_order(&self, input: &PlaceOrderInput) -> Result<serde_json::Value, String> {
        self.refuse_commit_if_gated("place_order")?;
        let body = json!({ "cart_id": input.cart_id });
        self.send_json(
            reqwest::Method::POST,
            "/v1/checkout",
            Some(&body),
            false,
            Some(&input.idempotency_key),
        )
        .await
    }

    /// `list_admin_products`.
    ///
    /// # Errors
    ///
    /// Returns transport, HTTP status, or JSON parse failures as a string.
    pub async fn list_admin_products(
        &self,
        input: &AdminListInput,
    ) -> Result<serde_json::Value, String> {
        let mut path = format!("/v1/{}/products", self.admin_prefix);
        let mut q = Vec::new();
        if let Some(limit) = input.limit {
            q.push(format!("limit={limit}"));
        }
        if let Some(offset) = input.offset {
            q.push(format!("offset={offset}"));
        }
        if !q.is_empty() {
            path.push('?');
            path.push_str(&q.join("&"));
        }
        self.get_json_admin(&path).await
    }

    /// `list_admin_orders`.
    ///
    /// # Errors
    ///
    /// Returns transport, HTTP status, or JSON parse failures as a string.
    pub async fn list_admin_orders(
        &self,
        input: &AdminListInput,
    ) -> Result<serde_json::Value, String> {
        let mut path = format!("/v1/{}/orders", self.admin_prefix);
        let mut q = Vec::new();
        if let Some(limit) = input.limit {
            q.push(format!("limit={limit}"));
        }
        if let Some(offset) = input.offset {
            q.push(format!("offset={offset}"));
        }
        if !q.is_empty() {
            path.push('?');
            path.push_str(&q.join("&"));
        }
        self.get_json_admin(&path).await
    }

    /// `patch_order_status`.
    ///
    /// # Errors
    ///
    /// Returns a commit-gate refusal, or transport / HTTP / JSON failures.
    pub async fn patch_order_status(
        &self,
        input: &PatchOrderStatusInput,
    ) -> Result<serde_json::Value, String> {
        self.refuse_commit_if_gated("patch_order_status")?;
        let body = json!({ "status": input.status });
        self.send_json(
            reqwest::Method::PATCH,
            &format!("/v1/{}/orders/{}", self.admin_prefix, input.id),
            Some(&body),
            true,
            None,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn commit_gate_refuses_without_allow() {
        let client = CommerceClient::new_for_test("http://127.0.0.1:9", false);
        let err = client
            .refuse_commit_if_gated("place_order")
            .expect_err("gated");
        assert!(err.contains(ALLOW_COMMIT_ENV));
        let ok = CommerceClient::new_for_test("http://127.0.0.1:9", true);
        ok.refuse_commit_if_gated("place_order").expect("allowed");
        ok.refuse_commit_if_gated("unknown_tool")
            .expect("unknown ok");
    }

    #[tokio::test]
    async fn transport_error_when_api_unreachable() {
        let client = CommerceClient::new_for_test("http://127.0.0.1:9", false);
        let err = client
            .list_products(&ListProductsInput {
                limit: Some(1),
                offset: None,
            })
            .await
            .expect_err("unreachable");
        assert_ne!(err, "");
    }

    #[test]
    fn from_env_reads_commit_and_restores_previous() {
        let _guard = ENV_LOCK.lock().expect("env lock");

        // Phase 1: prior values set → restore takes the `Some(v)` arms.
        // SAFETY: test-only env mutation; cleaned below.
        unsafe {
            std::env::set_var(API_BASE_ENV, "http://127.0.0.1:1/");
            std::env::set_var(ALLOW_COMMIT_ENV, "false");
            std::env::set_var(ADMIN_TOKEN_ENV, "prev-token");
            std::env::set_var(ADMIN_PREFIX_ENV, "prev-ops");
        }
        let prev_base = std::env::var(API_BASE_ENV).ok();
        let prev_commit = std::env::var(ALLOW_COMMIT_ENV).ok();
        let prev_token = std::env::var(ADMIN_TOKEN_ENV).ok();
        let prev_prefix = std::env::var(ADMIN_PREFIX_ENV).ok();
        unsafe {
            std::env::set_var(API_BASE_ENV, "http://127.0.0.1:18080/");
            std::env::set_var(ALLOW_COMMIT_ENV, "true");
            std::env::set_var(ADMIN_TOKEN_ENV, "secret");
            std::env::set_var(ADMIN_PREFIX_ENV, "ops");
        }
        let client = CommerceClient::from_env();
        assert!(client.allow_commit());
        restore_client_env(prev_base, prev_commit, prev_token, prev_prefix);
        assert_eq!(
            std::env::var(API_BASE_ENV).as_deref(),
            Ok("http://127.0.0.1:1/")
        );
        assert_eq!(std::env::var(ALLOW_COMMIT_ENV).as_deref(), Ok("false"));
        assert_eq!(std::env::var(ADMIN_TOKEN_ENV).as_deref(), Ok("prev-token"));
        assert_eq!(std::env::var(ADMIN_PREFIX_ENV).as_deref(), Ok("prev-ops"));

        // Phase 2: previously unset → restore takes the `remove_var` arms.
        unsafe {
            std::env::remove_var(API_BASE_ENV);
            std::env::remove_var(ALLOW_COMMIT_ENV);
            std::env::remove_var(ADMIN_TOKEN_ENV);
            std::env::remove_var(ADMIN_PREFIX_ENV);
        }
        let prev_base = std::env::var(API_BASE_ENV).ok();
        let prev_commit = std::env::var(ALLOW_COMMIT_ENV).ok();
        let prev_token = std::env::var(ADMIN_TOKEN_ENV).ok();
        let prev_prefix = std::env::var(ADMIN_PREFIX_ENV).ok();
        unsafe {
            std::env::set_var(API_BASE_ENV, "http://127.0.0.1:18081/");
            std::env::set_var(ALLOW_COMMIT_ENV, "1");
            std::env::set_var(ADMIN_TOKEN_ENV, "t");
            std::env::set_var(ADMIN_PREFIX_ENV, "ops");
        }
        let _ = CommerceClient::from_env();
        restore_client_env(prev_base, prev_commit, prev_token, prev_prefix);
        assert!(std::env::var(API_BASE_ENV).is_err());
        assert!(std::env::var(ALLOW_COMMIT_ENV).is_err());
        assert!(std::env::var(ADMIN_TOKEN_ENV).is_err());
        assert!(std::env::var(ADMIN_PREFIX_ENV).is_err());
    }

    fn restore_client_env(
        prev_base: Option<String>,
        prev_commit: Option<String>,
        prev_token: Option<String>,
        prev_prefix: Option<String>,
    ) {
        // SAFETY: called only from tests holding `ENV_LOCK`.
        unsafe {
            match prev_base {
                Some(v) => std::env::set_var(API_BASE_ENV, v),
                None => std::env::remove_var(API_BASE_ENV),
            }
            match prev_commit {
                Some(v) => std::env::set_var(ALLOW_COMMIT_ENV, v),
                None => std::env::remove_var(ALLOW_COMMIT_ENV),
            }
            match prev_token {
                Some(v) => std::env::set_var(ADMIN_TOKEN_ENV, v),
                None => std::env::remove_var(ADMIN_TOKEN_ENV),
            }
            match prev_prefix {
                Some(v) => std::env::set_var(ADMIN_PREFIX_ENV, v),
                None => std::env::remove_var(ADMIN_PREFIX_ENV),
            }
        }
    }
}
