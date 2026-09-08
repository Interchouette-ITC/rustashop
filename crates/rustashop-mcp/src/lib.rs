//! Axum MCP and shared commerce agent tool surfaces.
//!
//! This crate owns the **stable tool schema** (names, scopes, effects, input
//! shapes) shared by future MCP HTTP routes and in-app agents. It does not yet
//! listen on a socket; commerce execution stays on `rustashop-api`.

#![deny(missing_docs)]

mod tools;

pub use tools::{
    AddCartLineInput, AdminListInput, CartLineRefInput, CreateCartInput, GetCartInput,
    GetProductInput, ListProductsInput, PatchOrderStatusInput, PlaceOrderInput, TOOLS,
    ToolDescriptor, ToolEffect, ToolScope, add_cart_line_input_example, admin_list_input_example,
    create_cart_input_example, get_cart_input_example, get_product_input_example,
    list_products_input_example, patch_order_status_input_example, place_order_input_example,
    tool_by_name, tools_catalog_json, update_cart_line_input_example,
};

/// Crate name marker for workspace and diagnostics checks.
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
    fn crate_marker_and_kernel_status() {
        assert_eq!(MCP_CRATE, "rustashop-mcp");
        assert_eq!(kernel_status(), rustashop::kernel_status());
    }
}
