//! MCP server (`rmcp`) for rustashop commerce tools (stdio or Streamable HTTP).

#![allow(clippy::unused_async)]
#![allow(clippy::unused_async_trait_impl)]

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};

use crate::MCP_CRATE;
#[cfg(test)]
use crate::TOOLS;
use crate::client::CommerceClient;
use crate::tools::{
    AddCartLineInput, AdminListInput, CartLineRefInput, CreateCartInput, GetCartInput,
    GetProductInput, ListProductsInput, PatchOrderStatusInput, PlaceOrderInput,
};

/// Default Streamable HTTP bind (`RUSTASHOP_MCP_ADDR` / `--listen`).
pub const DEFAULT_HTTP_LISTEN: &str = "127.0.0.1:8090";

/// MCP server handle (clonable for Streamable HTTP sessions).
#[derive(Clone)]
pub struct RustashopMcp {
    client: CommerceClient,
}

impl Default for RustashopMcp {
    fn default() -> Self {
        Self::from_env()
    }
}

impl RustashopMcp {
    /// Builds a server using process environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            client: CommerceClient::from_env(),
        }
    }

    /// Builds a server with an explicit commerce client (tests).
    #[must_use]
    pub const fn with_client(client: CommerceClient) -> Self {
        Self { client }
    }
}

fn text_ok(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text.into())])
}

fn text_err(err: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(err.to_string())])
}

fn json_ok(value: &serde_json::Value) -> CallToolResult {
    text_ok(value.to_string())
}

#[tool_router]
impl RustashopMcp {
    /// `list_products` - `GET /v1/products`.
    #[tool(description = "List enabled catalog products with optional pagination")]
    async fn list_products(
        &self,
        Parameters(input): Parameters<ListProductsInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.list_products(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }

    /// `get_product` - `GET /v1/products/{id}`.
    #[tool(description = "Fetch one product and its variants by id")]
    async fn get_product(
        &self,
        Parameters(input): Parameters<GetProductInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.get_product(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }

    /// `create_cart` - `POST /v1/carts`.
    #[tool(description = "Create an open cart (optional ISO currency, default EUR)")]
    async fn create_cart(
        &self,
        Parameters(input): Parameters<CreateCartInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.create_cart(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }

    /// `get_cart` - `GET /v1/carts/{id}`.
    #[tool(description = "Read a cart by id including line totals")]
    async fn get_cart(
        &self,
        Parameters(input): Parameters<GetCartInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.get_cart(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }

    /// `add_cart_line` - `POST /v1/carts/{id}/lines`.
    #[tool(description = "Add or merge a variant line on an open cart")]
    async fn add_cart_line(
        &self,
        Parameters(input): Parameters<AddCartLineInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.add_cart_line(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }

    /// `update_cart_line` - `PATCH /v1/carts/{id}/lines/{line_id}`.
    #[tool(description = "Change quantity on an existing cart line")]
    async fn update_cart_line(
        &self,
        Parameters(input): Parameters<CartLineRefInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.update_cart_line(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }

    /// `delete_cart_line` - `DELETE /v1/carts/{id}/lines/{line_id}`.
    #[tool(description = "Remove a line from an open cart")]
    async fn delete_cart_line(
        &self,
        Parameters(input): Parameters<CartLineRefInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.delete_cart_line(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }

    /// `place_order` - `POST /v1/checkout` (commit; gated).
    #[tool(description = "Checkout a cart into a placed order (idempotent; commit gate)")]
    async fn place_order(
        &self,
        Parameters(input): Parameters<PlaceOrderInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.place_order(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }

    /// `list_admin_products` - admin bearer.
    #[tool(description = "List products for operators (admin bearer)")]
    async fn list_admin_products(
        &self,
        Parameters(input): Parameters<AdminListInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.list_admin_products(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }

    /// `list_admin_orders` - admin bearer.
    #[tool(description = "List orders for operators (admin bearer)")]
    async fn list_admin_orders(
        &self,
        Parameters(input): Parameters<AdminListInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.list_admin_orders(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }

    /// `patch_order_status` - admin commit gate.
    #[tool(description = "Update order lifecycle status (admin bearer; commit gate)")]
    async fn patch_order_status(
        &self,
        Parameters(input): Parameters<PatchOrderStatusInput>,
    ) -> Result<CallToolResult, McpError> {
        Ok(match self.client.patch_order_status(&input).await {
            Ok(v) => json_ok(&v),
            Err(err) => text_err(err),
        })
    }
}

fn http_router() -> axum::Router {
    let config =
        rmcp::transport::streamable_http_server::tower::StreamableHttpServerConfig::default();
    let service = rmcp::transport::streamable_http_server::tower::StreamableHttpService::new(
        || Ok(RustashopMcp::from_env()),
        Arc::new(
            rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default(),
        ),
        config,
    );
    let method_router = axum::routing::any_service(service);
    axum::Router::new()
        .route("/mcp", method_router.clone())
        .route("/mcp/", method_router)
}

async fn serve_listener(
    listener: tokio::net::TcpListener,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let addr = listener.local_addr()?;
    tracing::info!(%addr, crate = MCP_CRATE, "rustashop-mcp HTTP listening");
    axum::serve(listener, http_router())
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

/// Serves MCP over Streamable HTTP until the process is stopped.
///
/// # Errors
///
/// Returns I/O errors from binding or serving the Axum listener.
pub async fn run_http(addr: &str) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    serve_listener(listener, std::future::pending()).await
}

#[tool_handler]
impl ServerHandler for RustashopMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new(
                MCP_CRATE,
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "rustashop commerce MCP tools (catalog, cart, checkout, admin). Commit tools need RUSTASHOP_MCP_ALLOW_COMMIT=1. Proxies RUSTASHOP_API_BASE.",
            )
    }
}

#[cfg(test)]
impl RustashopMcp {
    async fn call_list_products(
        &self,
        input: ListProductsInput,
    ) -> Result<CallToolResult, McpError> {
        self.list_products(Parameters(input)).await
    }

    async fn call_get_product(&self, input: GetProductInput) -> Result<CallToolResult, McpError> {
        self.get_product(Parameters(input)).await
    }

    async fn call_create_cart(&self, input: CreateCartInput) -> Result<CallToolResult, McpError> {
        self.create_cart(Parameters(input)).await
    }

    async fn call_get_cart(&self, input: GetCartInput) -> Result<CallToolResult, McpError> {
        self.get_cart(Parameters(input)).await
    }

    async fn call_add_cart_line(
        &self,
        input: AddCartLineInput,
    ) -> Result<CallToolResult, McpError> {
        self.add_cart_line(Parameters(input)).await
    }

    async fn call_update_cart_line(
        &self,
        input: CartLineRefInput,
    ) -> Result<CallToolResult, McpError> {
        self.update_cart_line(Parameters(input)).await
    }

    async fn call_delete_cart_line(
        &self,
        input: CartLineRefInput,
    ) -> Result<CallToolResult, McpError> {
        self.delete_cart_line(Parameters(input)).await
    }

    async fn call_place_order(&self, input: PlaceOrderInput) -> Result<CallToolResult, McpError> {
        self.place_order(Parameters(input)).await
    }

    async fn call_list_admin_products(
        &self,
        input: AdminListInput,
    ) -> Result<CallToolResult, McpError> {
        self.list_admin_products(Parameters(input)).await
    }

    async fn call_list_admin_orders(
        &self,
        input: AdminListInput,
    ) -> Result<CallToolResult, McpError> {
        self.list_admin_orders(Parameters(input)).await
    }

    async fn call_patch_order_status(
        &self,
        input: PatchOrderStatusInput,
    ) -> Result<CallToolResult, McpError> {
        self.patch_order_status(Parameters(input)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::CommerceClient;
    use crate::tools::{
        AdminListInput, CartLineRefInput, GetProductInput, ListProductsInput,
        add_cart_line_input_example, admin_list_input_example, create_cart_input_example,
        get_cart_input_example, get_product_input_example, list_products_input_example,
        patch_order_status_input_example, place_order_input_example,
        update_cart_line_input_example,
    };
    use serde_json::json;
    use wiremock::matchers::{header, method, path, path_regex, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn tool_router_names_match_catalog() {
        let listed: Vec<String> = RustashopMcp::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        for desc in TOOLS {
            assert!(
                listed.iter().any(|n| n == desc.name),
                "missing MCP tool {}",
                desc.name
            );
        }
        assert_eq!(listed.len(), TOOLS.len());
    }

    #[test]
    fn mcp_server_version_matches_crate() {
        let info = RustashopMcp::default().get_info();
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.server_info.name.as_str(), MCP_CRATE);
    }

    #[tokio::test]
    async fn run_http_bind_failure_when_port_in_use() {
        let held = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("hold port");
        let addr = held.local_addr().expect("addr").to_string();
        let err = run_http(&addr).await;
        assert!(err.is_err(), "expected bind failure on busy port");
    }

    #[tokio::test]
    async fn run_http_pending_path_accepts_then_aborts() {
        let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("probe");
        let addr = probe.local_addr().expect("addr");
        drop(probe);

        let handle = tokio::spawn(async move { run_http(&addr.to_string()).await });

        for _ in 0..50 {
            match tokio::net::TcpStream::connect(addr).await {
                Ok(_) => break,
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            }
        }

        let slash = reqwest::Client::new()
            .post(format!("http://{addr}/mcp/"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body("{}")
            .send()
            .await
            .expect("slash post");
        assert_ne!(slash.status().as_u16(), 404);

        handle.abort();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn run_http_serves_mcp_and_shuts_down() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let server = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            serve_listener(listener, async {
                let _ = shutdown_rx.await;
            })
            .await
        });

        for _ in 0..50 {
            match tokio::net::TcpStream::connect(addr).await {
                Ok(_) => break,
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            }
        }

        let init = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "coverage-test", "version": "0.0.1" }
            }
        });
        let response = reqwest::Client::new()
            .post(format!("http://{addr}/mcp"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .json(&init)
            .send()
            .await
            .expect("mcp post");
        let status = response.status();
        assert!(status.is_success() || status.as_u16() == 406);

        let _ = shutdown_tx.send(());
        server
            .await
            .expect("join")
            .expect("serve_listener should shut down cleanly");
    }

    async fn mount_commerce_mocks(mock: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/v1/products"))
            .and(query_param("limit", "20"))
            .and(query_param("offset", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]})))
            .mount(mock)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"^/v1/products/[^/]+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"p1"})))
            .mount(mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/carts"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id":"c1"})))
            .mount(mock)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"^/v1/carts/[^/]+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"c1"})))
            .mount(mock)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/carts/[^/]+/lines$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok":true})))
            .mount(mock)
            .await;
        Mock::given(method("PATCH"))
            .and(path_regex(r"^/v1/carts/[^/]+/lines/[^/]+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"qty":3})))
            .mount(mock)
            .await;
        Mock::given(method("DELETE"))
            .and(path_regex(r"^/v1/carts/[^/]+/lines/[^/]+$"))
            .respond_with(ResponseTemplate::new(204))
            .mount(mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/checkout"))
            .and(header("Idempotency-Key", "agent-checkout-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"order_id":"o1"})))
            .mount(mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/ops/products"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]})))
            .mount(mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/ops/orders"))
            .and(query_param("limit", "20"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]})))
            .mount(mock)
            .await;
        Mock::given(method("PATCH"))
            .and(path_regex(r"^/v1/ops/orders/[^/]+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"shipped"})))
            .mount(mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/products"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(mock)
            .await;
    }

    async fn exercise_happy_path_tools(mcp: &RustashopMcp) {
        let ok = mcp
            .call_list_products(list_products_input_example())
            .await
            .expect("list");
        assert!(ok.is_error.is_none() || ok.is_error == Some(false));
        mcp.call_get_product(get_product_input_example())
            .await
            .expect("get product");
        mcp.call_create_cart(create_cart_input_example())
            .await
            .expect("create cart");
        mcp.call_get_cart(get_cart_input_example())
            .await
            .expect("get cart");
        mcp.call_add_cart_line(add_cart_line_input_example())
            .await
            .expect("add line");
        mcp.call_update_cart_line(update_cart_line_input_example())
            .await
            .expect("update line");
        let mut delete_line = update_cart_line_input_example();
        delete_line.quantity = None;
        mcp.call_delete_cart_line(delete_line)
            .await
            .expect("delete line");
        mcp.call_place_order(place_order_input_example())
            .await
            .expect("place");
        mcp.call_list_admin_products(AdminListInput {
            limit: None,
            offset: None,
        })
        .await
        .expect("admin products");
        mcp.call_list_admin_orders(admin_list_input_example())
            .await
            .expect("admin orders");
        mcp.call_patch_order_status(patch_order_status_input_example())
            .await
            .expect("patch");
    }

    #[tokio::test]
    async fn tools_proxy_commerce_api_via_wiremock() {
        let mock = MockServer::start().await;
        mount_commerce_mocks(&mock).await;

        let client = CommerceClient::new_for_test(mock.uri(), true).with_admin("tok", "ops");
        assert!(client.allow_commit());
        let mcp = RustashopMcp::with_client(client.clone());
        exercise_happy_path_tools(&mcp).await;

        let gated = RustashopMcp::with_client(CommerceClient::new_for_test(mock.uri(), false));
        let refused = gated
            .call_place_order(place_order_input_example())
            .await
            .expect("gated result");
        assert_eq!(refused.is_error, Some(true));

        let missing_qty = mcp
            .call_update_cart_line(CartLineRefInput {
                cart_id: "c".into(),
                line_id: "l".into(),
                quantity: None,
            })
            .await
            .expect("missing qty");
        assert_eq!(missing_qty.is_error, Some(true));

        let err = client
            .list_products(&ListProductsInput {
                limit: None,
                offset: None,
            })
            .await
            .expect_err("http 500");
        assert!(err.contains("500"));

        let bad_mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/products/x"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&bad_mock)
            .await;
        let bad = CommerceClient::new_for_test(bad_mock.uri(), false)
            .get_product(&GetProductInput { id: "x".into() })
            .await
            .expect_err("bad json");
        assert!(bad.contains("invalid JSON"));
    }
}
