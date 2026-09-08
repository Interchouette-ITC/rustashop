//! Axum MCP server exposing commerce tools from [`crate::TOOLS`].
//!
//! Streamable HTTP on `/mcp` (and `/mcp/`). Tool handlers proxy to the Actix
//! commerce API (`RUSTASHOP_API_BASE`). Commit-class tools require
//! `RUSTASHOP_MCP_ALLOW_COMMIT=1`.

#![allow(clippy::unused_async)]
#![allow(clippy::unused_async_trait_impl)]

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
};

use crate::MCP_CRATE;
#[cfg(test)]
use crate::TOOLS;
use crate::client::CommerceClient;
use crate::tools::{
    AddCartLineInput, AdminListInput, CartLineRefInput, CreateCartInput, GetCartInput,
    GetProductInput, ListProductsInput, PatchOrderStatusInput, PlaceOrderInput,
};

/// Default Streamable HTTP bind (`RUSTASHOP_MCP_BIND` / `--listen`).
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

/// Serves MCP over stdio until the client disconnects.
///
/// # Errors
///
/// Returns transport / protocol errors from rmcp.
pub async fn run_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = RustashopMcp::from_env();
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// Serves MCP over Streamable HTTP until the process is stopped.
///
/// # Errors
///
/// Returns I/O errors from binding or serving the Axum listener.
pub async fn run_http(addr: &str) -> std::io::Result<()> {
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
    let app = axum::Router::new()
        .route("/mcp", method_router.clone())
        .route("/mcp/", method_router)
        .route("/healthz", axum::routing::get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, crate = MCP_CRATE, "rustashop-mcp HTTP listening");
    axum::serve(listener, app).await?;
    Ok(())
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
mod tests {
    use super::*;

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
        let info = RustashopMcp::from_env().get_info();
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.server_info.name.as_str(), MCP_CRATE);
    }
}
