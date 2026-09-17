//! MCP server (`rmcp`) for rustashop commerce tools (stdio or Streamable HTTP).

#![allow(clippy::unused_async)]
#![allow(clippy::unused_async_trait_impl)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Request as HttpRequest};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use serenade_http::{AsyncHttpKernel, HttpError, Method, Request, Response, box_future};
use serenade_http_axum::{BoundServer, ShutdownHandle, await_bound, bind_server};
use tokio::net::ToSocketAddrs;
use tower::ServiceExt;

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

/// Max body accepted when bridging Streamable HTTP through the Serenade kernel.
const MAX_MCP_BODY_BYTES: usize = 16 * 1024 * 1024;

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

type McpStreamableService = rmcp::transport::streamable_http_server::tower::StreamableHttpService<
    RustashopMcp,
    rmcp::transport::streamable_http_server::session::local::LocalSessionManager,
>;

fn streamable_service() -> McpStreamableService {
    let config =
        rmcp::transport::streamable_http_server::tower::StreamableHttpServerConfig::default();
    rmcp::transport::streamable_http_server::tower::StreamableHttpService::new(
        || Ok(RustashopMcp::from_env()),
        Arc::new(
            rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default(),
        ),
        config,
    )
}

/// Serenade async kernel that bridges `/mcp` to Streamable HTTP.
#[must_use]
pub fn mcp_http_kernel() -> AsyncHttpKernel {
    let service = streamable_service();
    AsyncHttpKernel::from_async_fn(move |request: &mut Request| {
        let mut service = service.clone();
        let path = request.path().to_owned();
        let method = request.method();
        let query = request.query().map(str::to_owned);
        let headers: Vec<(String, String)> = request
            .headers()
            .iter()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect();
        let body = request.body().to_vec();
        box_future(async move {
            if path != "/mcp" && path != "/mcp/" {
                return Ok(Response::new(404).with_body(b"no handler".to_vec()));
            }
            bridge_streamable(
                &mut service,
                method,
                &path,
                query.as_deref(),
                &headers,
                body,
            )
            .await
        })
    })
}

async fn bridge_streamable(
    service: &mut McpStreamableService,
    method: Method,
    path: &str,
    query: Option<&str>,
    headers: &[(String, String)],
    body: Vec<u8>,
) -> Result<Response, HttpError> {
    let http_request = build_http_request(method, path, query, headers, body)?;
    let http_response = service
        .oneshot(http_request)
        .await
        .expect("streamable HTTP service is infallible");
    Ok(collect_http_response(http_response).await)
}

fn build_http_request(
    method: Method,
    path: &str,
    query: Option<&str>,
    headers: &[(String, String)],
    body: Vec<u8>,
) -> Result<HttpRequest<Body>, HttpError> {
    let uri = query
        .filter(|part| !part.is_empty())
        .map_or_else(|| path.to_owned(), |part| format!("{path}?{part}"));
    let http_method = axum::http::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|error| HttpError::status(405, error.to_string()))?;
    let mut builder = HttpRequest::builder().method(http_method).uri(uri);
    for (name, value) in headers {
        if let (Ok(header_name), Ok(header_value)) = (
            HeaderName::try_from(name.as_str()),
            HeaderValue::from_str(value),
        ) {
            builder = builder.header(header_name, header_value);
        }
    }
    builder
        .body(Body::from(body))
        .map_err(|error| HttpError::status(400, error.to_string()))
}

async fn collect_http_response<B>(response: axum::http::Response<B>) -> Response
where
    B: axum::body::HttpBody<Data = axum::body::Bytes> + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let status = response.status().as_u16();
    let header_pairs: Vec<(String, String)> = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|text| (name.as_str().to_owned(), text.to_owned()))
        })
        .collect();
    let body = axum::body::to_bytes(Body::new(response.into_body()), MAX_MCP_BODY_BYTES)
        .await
        .unwrap_or_default();
    let mut out = Response::new(status).with_body(body.to_vec());
    for (name, value) in header_pairs {
        out = out.with_header(name, value);
    }
    out
}

/// Binds Streamable MCP HTTP via Serenade Axum [`bind_server`] (tests / graceful stop).
///
/// # Errors
///
/// Propagates bind errors from the Serenade Axum helper.
pub async fn bind_http(addr: impl ToSocketAddrs) -> std::io::Result<(BoundServer, ShutdownHandle)> {
    let kernel = mcp_http_kernel();
    let (server, shutdown) = bind_server(addr, kernel).await?;
    let addr = server.local_addr();
    tracing::info!("rustashop-mcp HTTP listening on {addr} ({MCP_CRATE})");
    Ok((server, shutdown))
}

/// Serves MCP over Streamable HTTP until the process is stopped.
///
/// # Errors
///
/// Returns I/O errors from binding or serving through Serenade Axum.
pub async fn run_http(addr: &str) -> std::io::Result<()> {
    let (server, _shutdown) = bind_http(addr).await?;
    await_bound(server).await
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
        CartLineRefInput, GetProductInput, ListProductsInput, add_cart_line_input_example,
        admin_list_input_example, create_cart_input_example, get_cart_input_example,
        get_product_input_example, list_products_input_example, patch_order_status_input_example,
        place_order_input_example, update_cart_line_input_example,
    };
    use serde_json::json;
    use wiremock::matchers::{header, method, path, path_regex, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn poll_tcp_ready(addr: std::net::SocketAddr, attempts: u32) -> bool {
        for _ in 0..attempts {
            match tokio::net::TcpStream::connect(addr).await {
                Ok(_) => return true,
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            }
        }
        false
    }

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

        assert!(
            poll_tcp_ready(addr, 50).await,
            "mcp http should accept connections"
        );

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
    async fn bind_http_serves_mcp_and_shuts_down() {
        let (server, shutdown) = bind_http("127.0.0.1:0").await.expect("bind");
        let addr = server.local_addr();
        let join = tokio::spawn(async move { await_bound(server).await });

        assert!(
            poll_tcp_ready(addr, 50).await,
            "mcp http should accept connections"
        );

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

        let unknown = reqwest::Client::new()
            .get(format!("http://{addr}/nope"))
            .send()
            .await
            .expect("unknown");
        assert_eq!(unknown.status().as_u16(), 404);

        let with_query = reqwest::Client::new()
            .post(format!("http://{addr}/mcp?sessionId=cov"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .json(&init)
            .send()
            .await
            .expect("mcp post with query");
        assert_ne!(with_query.status().as_u16(), 404);

        shutdown.shutdown();
        join.await
            .expect("join")
            .expect("await_bound should shut down cleanly");
    }

    #[test]
    fn build_http_request_skips_invalid_headers_and_keeps_query() {
        let request = build_http_request(
            Method::Post,
            "/mcp",
            Some("sessionId=1"),
            &[
                ("content-type".into(), "application/json".into()),
                ("not a header".into(), "x".into()),
                ("x-bad".into(), "a\nb".into()),
            ],
            b"{}".to_vec(),
        )
        .expect("build");
        assert_eq!(request.uri().path(), "/mcp");
        assert_eq!(request.uri().query(), Some("sessionId=1"));
        assert!(request.headers().get("content-type").is_some());
        assert!(request.headers().get("x-bad").is_none());
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
            .and(query_param("limit", "20"))
            .and(query_param("offset", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]})))
            .mount(mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/ops/orders"))
            .and(query_param("limit", "20"))
            .and(query_param("offset", "0"))
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
        mcp.call_list_admin_products(admin_list_input_example())
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
    async fn tool_transport_errors_surface_as_text_err() {
        let mcp = RustashopMcp::with_client(
            CommerceClient::new_for_test("http://127.0.0.1:9", true).with_admin("tok", "ops"),
        );
        let results = [
            mcp.call_list_products(list_products_input_example())
                .await
                .expect("list"),
            mcp.call_get_product(get_product_input_example())
                .await
                .expect("get"),
            mcp.call_create_cart(create_cart_input_example())
                .await
                .expect("create"),
            mcp.call_get_cart(get_cart_input_example())
                .await
                .expect("get cart"),
            mcp.call_add_cart_line(add_cart_line_input_example())
                .await
                .expect("add"),
            mcp.call_delete_cart_line(update_cart_line_input_example())
                .await
                .expect("delete"),
            mcp.call_list_admin_products(admin_list_input_example())
                .await
                .expect("admin products"),
            mcp.call_list_admin_orders(admin_list_input_example())
                .await
                .expect("admin orders"),
            mcp.call_patch_order_status(patch_order_status_input_example())
                .await
                .expect("patch"),
        ];
        for result in results {
            assert_eq!(result.is_error, Some(true));
        }
    }

    #[tokio::test]
    async fn poll_tcp_ready_sleeps_when_refused() {
        let addr: std::net::SocketAddr = "127.0.0.1:9".parse().expect("addr");
        assert!(!poll_tcp_ready(addr, 2).await);
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
