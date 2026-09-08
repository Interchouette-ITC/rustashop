//! Admin AI tool catalog (`GET …/ai/tools`).

use rustashop_domain::{TOOLS, ToolDescriptor, ToolEffect, ToolScope};
use serde::{Deserialize, Serialize};
use serenade_http::Response;
use utoipa::ToSchema;

use crate::admin_auth::AdminAuthConfig;
use crate::error::{ErrorBody, api_error_json_response, json_response};

/// One tool row for admin agent discovery.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AiToolResponse {
    /// Stable MCP / agent function name.
    pub name: String,
    /// One-line capability summary.
    pub summary: String,
    /// Matching `OpenAPI` path template.
    pub openapi_path: String,
    /// HTTP method for the commerce route.
    pub method: String,
    /// Shop vs admin authz class (`shop` / `admin`).
    pub scope: String,
    /// Read / draft write / commit (`read` / `draft_write` / `commit`).
    pub effect: String,
    /// Autonomous agents need human or strict policy approve.
    pub human_approve_for_autonomous: bool,
}

impl From<&ToolDescriptor> for AiToolResponse {
    fn from(tool: &ToolDescriptor) -> Self {
        Self {
            name: tool.name.to_owned(),
            summary: tool.summary.to_owned(),
            openapi_path: tool.openapi_path.to_owned(),
            method: tool.method.to_owned(),
            scope: scope_label(tool.scope).to_owned(),
            effect: effect_label(tool.effect).to_owned(),
            human_approve_for_autonomous: tool.human_approve_for_autonomous,
        }
    }
}

const fn scope_label(scope: ToolScope) -> &'static str {
    match scope {
        ToolScope::Shop => "shop",
        ToolScope::Admin => "admin",
    }
}

const fn effect_label(effect: ToolEffect) -> &'static str {
    match effect {
        ToolEffect::Read => "read",
        ToolEffect::DraftWrite => "draft_write",
        ToolEffect::Commit => "commit",
    }
}

/// Lists the shared commerce tool catalog for in-app admin agents.
pub fn list_ai_tools_response(auth: &AdminAuthConfig, bearer: Option<&str>) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    let tools: Vec<AiToolResponse> = TOOLS.iter().map(AiToolResponse::from).collect();
    json_response(200, &tools)
}

/// `GET /v1/{admin_api_prefix}/ai/tools` `OpenAPI` stub.
#[utoipa::path(
    get,
    path = "/v1/{admin_api_prefix}/ai/tools",
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Commerce AI tool catalog", body = [AiToolResponse]),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn list_ai_tools() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthorized_without_bearer() {
        let auth = AdminAuthConfig::from_token("tok");
        assert_eq!(list_ai_tools_response(&auth, None).status(), 401);
        assert_eq!(list_ai_tools_response(&auth, Some("wrong")).status(), 401);
    }

    #[test]
    fn lists_catalog_with_bearer() {
        let auth = AdminAuthConfig::from_token("tok");
        let response = list_ai_tools_response(&auth, Some("tok"));
        assert_eq!(response.status(), 200);
        let body: Vec<AiToolResponse> =
            serde_json::from_slice(response.body()).expect("ai tools json");
        assert_eq!(body.len(), TOOLS.len());
        assert!(body.iter().any(|t| t.name == "list_admin_products"));
        list_ai_tools();
    }
}
