//! Axum MCP and shared commerce agent tool surfaces.
//!
//! This crate exposes MCP (stdio or Streamable HTTP on `/mcp`) over the shared
//! commerce tool catalog in `rustashop-domain` (`TOOLS`), proxying to
//! `rustashop-api`. Commerce execution stays on the Actix API.

#![deny(missing_docs)]

mod client;
mod server;
mod tools;

pub use client::{
    ADMIN_PREFIX_ENV, ADMIN_TOKEN_ENV, ALLOW_COMMIT_ENV, API_BASE_ENV, CommerceClient,
};
pub use server::{DEFAULT_HTTP_LISTEN, RustashopMcp, run_http};
pub use tools::{
    AddCartLineInput, AdminListInput, CartLineRefInput, CreateCartInput, GetCartInput,
    GetProductInput, ListProductsInput, PatchOrderStatusInput, PlaceOrderInput, TOOLS,
    ToolDescriptor, ToolEffect, ToolScope, add_cart_line_input_example, admin_list_input_example,
    create_cart_input_example, get_cart_input_example, get_product_input_example,
    list_products_input_example, patch_order_status_input_example, place_order_input_example,
    tool_by_name, tools_catalog_json, update_cart_line_input_example,
};

/// Crate id for workspace and diagnostics checks.
pub const MCP_CRATE: &str = "rustashop-mcp";

/// Kernel integration status from the `rustashop` application package.
#[must_use]
pub fn kernel_status() -> &'static str {
    rustashop::kernel_status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_id_and_kernel_status() {
        assert_eq!(MCP_CRATE, "rustashop-mcp");
        assert_eq!(kernel_status(), rustashop::kernel_status());
    }
}
