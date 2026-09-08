//! Commerce AI tool input schemas (MCP / agents).
//!
//! The stable catalog (`TOOLS`) lives in `rustashop-domain` so Actix and Axum
//! share one definition.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use rustashop_domain::{
    TOOLS, ToolDescriptor, ToolEffect, ToolScope, tool_by_name, tools_catalog_json,
};

/// Input for `list_products`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ListProductsInput {
    /// Maximum rows (commerce caps at 100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Rows to skip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// Input for `get_product`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct GetProductInput {
    /// Product id.
    pub id: String,
}

/// Input for `create_cart`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct CreateCartInput {
    /// ISO currency (default `EUR` when omitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

/// Input for `get_cart`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct GetCartInput {
    /// Cart id.
    pub id: String,
}

/// Input for `add_cart_line`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct AddCartLineInput {
    /// Cart id (path `{id}`).
    pub cart_id: String,
    /// Variant to add.
    pub variant_id: String,
    /// Quantity greater than zero.
    pub quantity: i32,
}

/// Input for `update_cart_line` / `delete_cart_line` path params.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct PlaceOrderInput {
    /// Cart to check out.
    pub cart_id: String,
    /// Idempotency key (also sent as `Idempotency-Key` on HTTP).
    pub idempotency_key: String,
}

/// Input for admin list tools (`list_admin_products` / `list_admin_orders`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct AdminListInput {
    /// Maximum rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Rows to skip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// Input for `patch_order_status`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
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
pub fn update_cart_line_input_example() -> CartLineRefInput {
    CartLineRefInput {
        cart_id: "00000000-0000-4000-8000-0000000000c1".into(),
        line_id: "00000000-0000-4000-8000-0000000000l1".into(),
        quantity: Some(3),
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
pub const fn admin_list_input_example() -> AdminListInput {
    AdminListInput {
        limit: Some(20),
        offset: Some(0),
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
