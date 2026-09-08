//! Coverage and contract tests for the v0 commerce AI tool schema.

use rustashop_mcp::{
    TOOLS, ToolEffect, ToolScope, add_cart_line_input_example, admin_list_input_example,
    create_cart_input_example, get_cart_input_example, get_product_input_example,
    list_products_input_example, patch_order_status_input_example, place_order_input_example,
    tool_by_name, tools_catalog_json, update_cart_line_input_example,
};

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
    assert_eq!(
        tool_by_name("list_products").map(|t| t.scope),
        Some(ToolScope::Shop)
    );
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
fn all_input_examples_roundtrip_json() {
    let cases: Vec<serde_json::Value> = vec![
        serde_json::to_value(list_products_input_example()).expect("list_products"),
        serde_json::to_value(get_product_input_example()).expect("get_product"),
        serde_json::to_value(create_cart_input_example()).expect("create_cart"),
        serde_json::to_value(get_cart_input_example()).expect("get_cart"),
        serde_json::to_value(add_cart_line_input_example()).expect("add_cart_line"),
        serde_json::to_value(update_cart_line_input_example()).expect("update_cart_line"),
        serde_json::to_value(place_order_input_example()).expect("place_order"),
        serde_json::to_value(admin_list_input_example()).expect("admin_list"),
        serde_json::to_value(patch_order_status_input_example()).expect("patch_order"),
    ];
    assert_eq!(cases.len(), 9);
    assert_eq!(
        place_order_input_example().idempotency_key,
        "agent-checkout-1"
    );
    assert_eq!(update_cart_line_input_example().quantity, Some(3));
}

#[test]
fn tools_catalog_snapshot() {
    let value = tools_catalog_json().expect("json");
    insta::assert_json_snapshot!("tools_catalog_v0", value);
}
