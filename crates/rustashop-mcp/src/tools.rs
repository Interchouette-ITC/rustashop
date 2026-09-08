//! Commerce AI tool schema aligned with `OpenAPI` resources.
//!
//! Tools are host-mediated: no SQL-from-prompt, integer money only, and
//! commit-class effects require stronger policy than cart draft writes.

use serde::{Deserialize, Serialize};

/// Who may invoke the tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolScope {
    /// Customer / shop session surface (catalog, cart, checkout).
    Shop,
    /// Operator surface (admin bearer + opaque URI prefix).
    Admin,
}

/// Side effect class for policy gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Input for `list_products`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListProductsInput {
    /// Maximum rows (commerce caps at 100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Rows to skip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// Input for `get_product`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetProductInput {
    /// Product id.
    pub id: String,
}

/// Input for `create_cart`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateCartInput {
    /// ISO currency (default `EUR` when omitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

/// Input for `get_cart`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetCartInput {
    /// Cart id.
    pub id: String,
}

/// Input for `add_cart_line`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddCartLineInput {
    /// Cart id (path `{id}`).
    pub cart_id: String,
    /// Variant to add.
    pub variant_id: String,
    /// Quantity greater than zero.
    pub quantity: i32,
}

/// Input for `update_cart_line` / `delete_cart_line` path params.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CartLineRefInput {
    /// Cart id.
    pub cart_id: String,
    /// Line id.
    pub line_id: String,
    /// Replacement quantity (update only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<i32>,
}

/// Input for `place_order`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaceOrderInput {
    /// Cart to check out.
    pub cart_id: String,
    /// Idempotency key (also sent as `Idempotency-Key` on HTTP).
    pub idempotency_key: String,
}

/// Input for admin list tools (`list_admin_products` / `list_admin_orders`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminListInput {
    /// Maximum rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Rows to skip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// Input for `patch_order_status`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchOrderStatusInput {
    /// Order id.
    pub id: String,
    /// Target status (`pending`, `paid`, `shipped`, `cancelled`, …).
    pub status: String,
}

/// Example input for docs and tests.
#[must_use]
pub const fn list_products_input_example() -> ListProductsInput {
    ListProductsInput {
        limit: Some(20),
        offset: Some(0),
    }
}

/// Example input for docs and tests.
#[must_use]
pub fn get_product_input_example() -> GetProductInput {
    GetProductInput {
        id: "00000000-0000-4000-8000-000000000001".into(),
    }
}

/// Example input for docs and tests.
#[must_use]
pub fn create_cart_input_example() -> CreateCartInput {
    CreateCartInput {
        currency: Some("EUR".into()),
    }
}

/// Example input for docs and tests.
#[must_use]
pub fn get_cart_input_example() -> GetCartInput {
    GetCartInput {
        id: "00000000-0000-4000-8000-0000000000c1".into(),
    }
}

/// Example input for docs and tests.
#[must_use]
pub fn add_cart_line_input_example() -> AddCartLineInput {
    AddCartLineInput {
        cart_id: "00000000-0000-4000-8000-0000000000c1".into(),
        variant_id: "00000000-0000-4000-8000-0000000000v1".into(),
        quantity: 2,
    }
}

/// Example input for docs and tests.
#[must_use]
pub fn place_order_input_example() -> PlaceOrderInput {
    PlaceOrderInput {
        cart_id: "00000000-0000-4000-8000-0000000000c1".into(),
        idempotency_key: "agent-checkout-1".into(),
    }
}

/// Example input for docs and tests.
#[must_use]
pub fn patch_order_status_input_example() -> PatchOrderStatusInput {
    PatchOrderStatusInput {
        id: "00000000-0000-4000-8000-0000000000o1".into(),
        status: "shipped".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_unique_name() {
        let mut names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), TOOLS.len());
    }

    #[test]
    fn commit_tools_require_human_approve_for_autonomous() {
        for tool in TOOLS {
            if tool.effect == ToolEffect::Commit {
                assert!(
                    tool.human_approve_for_autonomous,
                    "{} commit tool must gate autonomous runs",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn tool_by_name_finds_catalog_and_misses_unknown() {
        assert_eq!(tool_by_name("list_products").map(|t| t.method), Some("GET"));
        assert!(tool_by_name("run_raw_sql").is_none());
    }

    #[test]
    fn catalog_json_lists_expected_names() {
        let value = tools_catalog_json().expect("json");
        let names: Vec<&str> = value
            .as_array()
            .expect("array")
            .iter()
            .map(|row| row["name"].as_str().expect("name"))
            .collect();
        assert!(names.contains(&"list_products"));
        assert!(names.contains(&"place_order"));
        assert!(names.contains(&"patch_order_status"));
        assert!(!names.iter().any(|n| n.contains("sql")));
    }

    #[test]
    fn input_examples_roundtrip_json() {
        let line = add_cart_line_input_example();
        let back: AddCartLineInput =
            serde_json::from_value(serde_json::to_value(&line).expect("ser")).expect("de");
        assert_eq!(back, line);
        assert_eq!(
            place_order_input_example().idempotency_key,
            "agent-checkout-1"
        );
    }

    #[test]
    fn tools_catalog_snapshot() {
        let value = tools_catalog_json().expect("json");
        insta::assert_json_snapshot!("tools_catalog_v0", value);
    }
}
