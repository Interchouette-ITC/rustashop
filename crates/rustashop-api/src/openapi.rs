//! `OpenAPI` document for the Actix commerce API.

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::admin_orders::{OrderListResponse, PatchOrderStatusRequest};
use crate::carts::{
    AddCartLineRequest, CartLineResponse, CartResponse, CreateCartRequest, MoneyResponse,
    UpdateCartLineRequest,
};
use crate::checkout::{CheckoutRequest, OrderLineResponse, OrderResponse};
use crate::error::{ErrorBody, json_response};
use crate::health::HealthResponse;
use crate::products::{
    ProductDetailResponse, ProductListResponse, ProductResponse, ProductVariantResponse,
};

struct AdminSecurityAddon;

impl Modify for AdminSecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "admin_bearer",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("token")
                        .build(),
                ),
            );
        }
    }
}

/// Generated `OpenAPI` document.
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::health::healthz,
        crate::products::list_products,
        crate::products::get_product,
        crate::carts::create_cart,
        crate::carts::get_cart,
        crate::carts::add_cart_line,
        crate::carts::update_cart_line,
        crate::carts::delete_cart_line,
        crate::checkout::place_order,
        crate::admin_orders::list_admin_orders,
        crate::admin_orders::patch_admin_order,
        crate::admin_products::list_admin_products,
        crate::sandbox_jobs::create_sandbox_job,
        crate::sandbox_jobs::get_sandbox_job,
        crate::sandbox_jobs::list_sandbox_audit,
        crate::sandbox_autonomous::commit_sandbox_job,
        crate::sandbox_autonomous::discard_sandbox_job,
        crate::ai_tools::list_ai_tools,
        crate::ai_tools::list_shop_ai_tools,
        crate::model_providers::list_ai_providers,
        crate::model_providers::list_ai_providers_catalog,
        crate::model_providers::test_ai_provider,
        openapi_json
    ),
    components(schemas(
        HealthResponse,
        ProductResponse,
        ProductDetailResponse,
        ProductVariantResponse,
        ProductListResponse,
        CartResponse,
        CartLineResponse,
        MoneyResponse,
        CreateCartRequest,
        AddCartLineRequest,
        UpdateCartLineRequest,
        CheckoutRequest,
        OrderResponse,
        OrderLineResponse,
        OrderListResponse,
        PatchOrderStatusRequest,
        crate::sandbox_jobs::CreateSandboxJobRequest,
        crate::sandbox_jobs::SandboxJobLine,
        crate::sandbox_jobs::SandboxJobResponse,
        crate::sandbox_jobs::SandboxJobStatus,
        crate::sandbox_jobs::SandboxAdjustmentResponse,
        crate::sandbox_jobs::SandboxProposalResponse,
        crate::sandbox_jobs::SandboxAuditRecord,
        crate::sandbox_autonomous::CommitSandboxJobResponse,
        crate::ai_tools::AiToolResponse,
        crate::model_providers::AiCredentialSource,
        crate::model_providers::AiProviderCatalogItem,
        crate::model_providers::AiProviderStatus,
        crate::model_providers::AiProvidersStatusResponse,
        crate::model_providers::AiProvidersCatalogResponse,
        crate::model_providers::AiProviderTestRequest,
        crate::model_providers::AiProviderTestResponse,
        ErrorBody
    )),
    tags(
        (name = "health", description = "Process liveness"),
        (name = "openapi", description = "Published OpenAPI document"),
        (name = "products", description = "Storefront catalog"),
        (name = "carts", description = "Cart CRUD"),
        (name = "checkout", description = "Place order from cart"),
        (name = "admin-orders", description = "Operator order list and status"),
        (name = "admin-products", description = "Operator product list"),
        (name = "sandbox", description = "Sandbox jobs and audit"),
        (name = "ai-tools", description = "Commerce AI tool catalog"),
        (name = "ai-providers", description = "Model provider status and probes")
    ),
    modifiers(&AdminSecurityAddon)
)]
pub struct ApiDoc;

/// Builds the commerce `OpenAPI` document with title, version, and optional server.
#[must_use]
pub fn published_openapi(server_url: Option<&str>) -> utoipa::openapi::OpenApi {
    let mut openapi = ApiDoc::openapi();
    let description = openapi.info.description.clone();
    let contact = openapi.info.contact.clone();
    let license = openapi.info.license.clone();
    serenade_openapi::finalize_openapi(
        &mut openapi,
        "rustashop-api",
        env!("CARGO_PKG_VERSION"),
        server_url,
    );
    openapi.info.description = description;
    openapi.info.contact = contact;
    openapi.info.license = license;
    openapi
}

/// Serenade JSON body for `GET /openapi.json`.
#[must_use]
pub fn openapi_json_response() -> serenade_http::Response {
    json_response(200, &published_openapi(None))
}

/// `GET /openapi.json` `OpenAPI` path (served by the Serenade HTTP front controller).
#[utoipa::path(
    get,
    path = "/openapi.json",
    tag = "openapi",
    responses((status = 200, description = "OpenAPI document"))
)]
#[allow(clippy::missing_const_for_fn)]
pub fn openapi_json() {}

/// Registers explorer UIs (feature `openapi-ui`) on an Actix service config.
#[cfg(feature = "openapi-ui")]
pub fn configure_openapi_ui(cfg: &mut actix_web::web::ServiceConfig, server_url: Option<&str>) {
    let openapi = published_openapi(server_url);
    let paths = serenade_openapi::OpenApiUiPaths::new();
    serenade_openapi::configure_actix_ui(cfg, openapi, &paths);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_stub_is_callable() {
        openapi_json();
    }

    #[test]
    fn openapi_json_response_is_ok() {
        assert_eq!(openapi_json_response().status(), 200);
    }

    #[test]
    fn published_openapi_sets_info() {
        let doc = published_openapi(Some("http://127.0.0.1:8080"));
        assert_eq!(doc.info.title, "rustashop-api");
        assert_ne!(doc.info.version, "");
        let servers = doc.servers.expect("servers");
        assert_eq!(servers[0].url, "http://127.0.0.1:8080");
    }

    #[test]
    fn openapi_uses_human_tags() {
        let doc = ApiDoc::openapi();
        let tags = doc.tags.expect("document tags");
        let names: Vec<&str> = tags.iter().map(|tag| tag.name.as_str()).collect();
        assert!(names.contains(&"products"));
        assert!(names.contains(&"carts"));
        assert!(names.iter().all(|name| !name.contains("::")));
        let product_get = doc
            .paths
            .paths
            .get("/v1/products")
            .and_then(|item| item.get.as_ref())
            .expect("GET /v1/products");
        assert_eq!(
            product_get.tags.as_deref(),
            Some(["products".to_owned()].as_slice())
        );
    }
}
