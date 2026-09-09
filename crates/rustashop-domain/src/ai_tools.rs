//! Commerce AI tool catalog shared by MCP and in-app agent UIs.
//!
//! Tools are host-mediated: no SQL-from-prompt, integer money only, and
//! commit-class effects require stronger policy than cart draft writes.

use serde::Serialize;

/// Who may invoke the tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolScope {
    /// Customer / shop session surface (catalog, cart, checkout).
    Shop,
    /// Operator surface (admin bearer + opaque URI prefix).
    Admin,
}

/// Side effect class for policy gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    /// Pure read of commerce state.
    Read,
    /// Mutates draft session state (open cart lines). Not payment capture.
    DraftWrite,
    /// Places an order or changes order lifecycle; host-authorized commit.
    Commit,
}

/// One callable tool shared by MCP and in-app agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ToolDescriptor {
    /// Stable MCP / agent function name (`snake_case`).
    pub name: &'static str,
    /// One-line capability summary.
    pub summary: &'static str,
    /// Matching `OpenAPI` path template.
    pub openapi_path: &'static str,
    /// HTTP method for the commerce route.
    pub method: &'static str,
    /// Shop vs admin authz class.
    pub scope: ToolScope,
    /// Read / draft write / commit.
    pub effect: ToolEffect,
    /// Autonomous agents must not run this without human or strict policy approve.
    pub human_approve_for_autonomous: bool,
}

/// v0 tool catalog (catalog, cart, checkout, admin order/product reads + status).
pub const TOOLS: &[ToolDescriptor] = &[
    ToolDescriptor {
        name: "list_products",
        summary: "List enabled catalog products with optional pagination",
        openapi_path: "/v1/products",
        method: "GET",
        scope: ToolScope::Shop,
        effect: ToolEffect::Read,
        human_approve_for_autonomous: false,
    },
    ToolDescriptor {
        name: "get_product",
        summary: "Fetch one product and its variants by id",
        openapi_path: "/v1/products/{id}",
        method: "GET",
        scope: ToolScope::Shop,
        effect: ToolEffect::Read,
        human_approve_for_autonomous: false,
    },
    ToolDescriptor {
        name: "create_cart",
        summary: "Create an open cart (optional ISO currency, default EUR)",
        openapi_path: "/v1/carts",
        method: "POST",
        scope: ToolScope::Shop,
        effect: ToolEffect::DraftWrite,
        human_approve_for_autonomous: false,
    },
    ToolDescriptor {
        name: "get_cart",
        summary: "Read a cart by id including line totals",
        openapi_path: "/v1/carts/{id}",
        method: "GET",
        scope: ToolScope::Shop,
        effect: ToolEffect::Read,
        human_approve_for_autonomous: false,
    },
    ToolDescriptor {
        name: "add_cart_line",
        summary: "Add or merge a variant line on an open cart",
        openapi_path: "/v1/carts/{id}/lines",
        method: "POST",
        scope: ToolScope::Shop,
        effect: ToolEffect::DraftWrite,
        human_approve_for_autonomous: false,
    },
    ToolDescriptor {
        name: "update_cart_line",
        summary: "Change quantity on an existing cart line",
        openapi_path: "/v1/carts/{id}/lines/{line_id}",
        method: "PATCH",
        scope: ToolScope::Shop,
        effect: ToolEffect::DraftWrite,
        human_approve_for_autonomous: false,
    },
    ToolDescriptor {
        name: "delete_cart_line",
        summary: "Remove a line from an open cart",
        openapi_path: "/v1/carts/{id}/lines/{line_id}",
        method: "DELETE",
        scope: ToolScope::Shop,
        effect: ToolEffect::DraftWrite,
        human_approve_for_autonomous: false,
    },
    ToolDescriptor {
        name: "place_order",
        summary: "Checkout a cart into a placed order (idempotent)",
        openapi_path: "/v1/checkout",
        method: "POST",
        scope: ToolScope::Shop,
        effect: ToolEffect::Commit,
        human_approve_for_autonomous: true,
    },
    ToolDescriptor {
        name: "list_admin_products",
        summary: "List products for operators (admin bearer)",
        openapi_path: "/v1/{admin_api_prefix}/products",
        method: "GET",
        scope: ToolScope::Admin,
        effect: ToolEffect::Read,
        human_approve_for_autonomous: false,
    },
    ToolDescriptor {
        name: "list_admin_orders",
        summary: "List orders for operators (admin bearer)",
        openapi_path: "/v1/{admin_api_prefix}/orders",
        method: "GET",
        scope: ToolScope::Admin,
        effect: ToolEffect::Read,
        human_approve_for_autonomous: false,
    },
    ToolDescriptor {
        name: "patch_order_status",
        summary: "Update order lifecycle status (admin bearer)",
        openapi_path: "/v1/{admin_api_prefix}/orders/{id}",
        method: "PATCH",
        scope: ToolScope::Admin,
        effect: ToolEffect::Commit,
        human_approve_for_autonomous: true,
    },
];

/// Looks up a tool by stable name.
#[must_use]
pub fn tool_by_name(name: &str) -> Option<&'static ToolDescriptor> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// Serializes the tool catalog for MCP `list_tools` / agent discovery.
///
/// # Errors
///
/// Returns [`serde_json::Error`] when serialization fails.
pub fn tools_catalog_json() -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(TOOLS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_lookup_and_json() {
        assert!(tool_by_name("list_admin_products").is_some());
        assert!(tool_by_name("missing").is_none());
        let value = tools_catalog_json().expect("catalog json");
        assert_eq!(value.as_array().expect("array").len(), TOOLS.len());
    }
}
