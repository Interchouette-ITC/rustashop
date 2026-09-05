//! Actix-web API surface.

mod admin_auth;
mod admin_orders;
mod admin_prefix;
mod admin_products;
mod carts;
mod checkout;
mod error;
mod health;
mod http_front;
mod install_env;
mod install_fs;
mod install_routes;
mod openapi;
mod products;
mod request_param;

use actix_web::web;

pub use admin_auth::{AdminAuthConfig, ADMIN_TOKEN_ENV, ADMIN_TOKEN_ENV_ALT};
pub use admin_orders::{
    list_admin_orders, patch_admin_order, OrderListResponse, PatchOrderStatusRequest,
};
pub use admin_prefix::{
    configure_admin_routes, AdminApiPrefix, ADMIN_API_PREFIX_ENV, DEFAULT_ADMIN_API_PREFIX,
};
pub use admin_products::list_admin_products;
pub use carts::{
    add_cart_line, create_cart, delete_cart_line, get_cart, update_cart_line, CartLineResponse,
    CartResponse, MoneyResponse,
};
pub use checkout::{place_order, OrderLineResponse, OrderResponse};
pub use health::{health_json_body, healthz, HealthResponse};
pub use http_front::{commerce_http_kernel, configure_serenade_front, serenade_dispatch};
pub use install_env::{
    run_install_write, InstallEnvError, InstallWriteOptions, InstallWriteResult,
};
pub use install_fs::{
    install_artefacts_present, shop_root, INSTALL_DIR_NAME, INSTALL_OFF_DIR_NAME, ROOT_ENV,
};
pub use openapi::{openapi_json, swagger_ui, ApiDoc};
pub use products::{
    get_product, list_products, ProductDetailResponse, ProductListResponse, ProductResponse,
    ProductVariantResponse,
};

/// Default bind address when `RUSTASHOP_BIND` is unset.
pub const DEFAULT_BIND: &str = "127.0.0.1:8080";

/// Environment variable for the API listen address.
pub const BIND_ENV: &str = "RUSTASHOP_BIND";

/// Returns the bind address from `RUSTASHOP_BIND` or [`DEFAULT_BIND`].
#[must_use]
pub fn bind_address() -> String {
    std::env::var(BIND_ENV).unwrap_or_else(|_| DEFAULT_BIND.to_owned())
}

/// Registers HTTP routes on `cfg` (admin prefix from env / local default).
pub fn routes(cfg: &mut web::ServiceConfig) {
    configure_app(cfg, &AdminApiPrefix::from_env());
}

/// Registers HTTP routes with an explicit operator API prefix.
///
/// Serenade-fronted routes (`/healthz`, `/v1/products*`) are registered via
/// [`configure_serenade_front`].
pub fn configure_routes(cfg: &mut web::ServiceConfig, admin_prefix: &AdminApiPrefix) {
    cfg.service(openapi_json)
        .service(swagger_ui())
        .service(create_cart)
        .service(get_cart)
        .service(add_cart_line)
        .service(update_cart_line)
        .service(delete_cart_line)
        .service(place_order);
    configure_admin_routes(cfg, admin_prefix);
    install_routes::configure_install_from_env(cfg);
}

/// Registers Serenade front routes then leftover Actix commerce routes.
pub fn configure_app(cfg: &mut web::ServiceConfig, admin_prefix: &AdminApiPrefix) {
    configure_serenade_front(cfg);
    configure_routes(cfg, admin_prefix);
}

#[cfg(test)]
mod bind_tests {
    use super::{bind_address, BIND_ENV, DEFAULT_BIND};

    #[test]
    fn bind_address_defaults_when_unset() {
        unsafe {
            std::env::remove_var(BIND_ENV);
        }
        assert_eq!(bind_address(), DEFAULT_BIND);
    }

    #[test]
    fn bind_address_reads_env_override() {
        unsafe {
            std::env::set_var(BIND_ENV, "127.0.0.1:18080");
        }
        assert_eq!(bind_address(), "127.0.0.1:18080");
        unsafe {
            std::env::remove_var(BIND_ENV);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn healthz_returns_ok_json() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(commerce_http_kernel(None)))
                .configure(routes),
        )
        .await;
        let req = test::TestRequest::get().uri("/healthz").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let body: HealthResponse = test::read_body_json(resp).await;
        assert_eq!(body.status, "ok");
        assert_eq!(body.kernel, rustashop::kernel_status());
        insta::assert_json_snapshot!("healthz_body", body);
    }

    #[actix_web::test]
    async fn swagger_ui_serves_html() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(commerce_http_kernel(None)))
                .configure(routes),
        )
        .await;
        let req = test::TestRequest::get().uri("/swagger-ui/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn openapi_json_lists_product_paths() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(commerce_http_kernel(None)))
                .configure(routes),
        )
        .await;
        let req = test::TestRequest::get().uri("/openapi.json").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body: serde_json::Value = test::read_body_json(resp).await;
        let paths = body.get("paths").expect("paths");
        assert!(paths.get("/v1/products").is_some());
        assert!(paths.get("/v1/carts").is_some());
        assert!(paths.get("/v1/checkout").is_some());
        assert!(paths.get("/v1/{admin_api_prefix}/orders").is_some());
        assert!(paths.get("/v1/{admin_api_prefix}/products").is_some());
        assert!(paths.get("/healthz").is_some());
    }

    #[actix_web::test]
    async fn admin_routes_respect_custom_prefix() {
        let prefix = AdminApiPrefix::parse("bk-test1").expect("prefix");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(commerce_http_kernel(None)))
                .app_data(web::Data::new(AdminAuthConfig::from_token("tok")))
                .configure(|cfg| configure_app(cfg, &prefix)),
        )
        .await;

        let legacy = test::TestRequest::get()
            .uri("/v1/admin/products")
            .insert_header(("Authorization", "Bearer tok"))
            .to_request();
        let legacy_resp = test::call_service(&app, legacy).await;
        assert_eq!(legacy_resp.status(), 404);

        let custom = test::TestRequest::get()
            .uri("/v1/bk-test1/products")
            .insert_header(("Authorization", "Bearer tok"))
            .to_request();
        let custom_resp = test::call_service(&app, custom).await;
        // No catalog app_data => handler may 500; route must match (not 404).
        assert_ne!(custom_resp.status(), 404);
    }

    #[actix_web::test]
    async fn install_absent_without_dist() {
        let dir = std::env::temp_dir().join(format!("rs-no-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(commerce_http_kernel(None)))
                .configure(|cfg| {
                    install_routes::configure_install(cfg, &dir);
                    configure_serenade_front(cfg);
                }),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/install/api/status")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn install_status_when_dist_present() {
        let dir = std::env::temp_dir().join(format!("rs-yes-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let index = dir.join("install/dist/index.html");
        std::fs::create_dir_all(index.parent().unwrap()).expect("mkdir");
        std::fs::write(&index, "<!doctype html><title>i</title>").expect("write");
        let app = test::init_service(
            App::new().configure(|cfg| install_routes::configure_install(cfg, &dir)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/install/api/status")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["available"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod domain_smoke {
    #[test]
    fn money_compiles_in_api_crate() {
        let currency = rustashop_domain::Currency::new("EUR").expect("EUR");
        let money = rustashop_domain::Money::new(2500, currency);
        assert_eq!(money.amount_minor, 2500);
        assert_eq!(money.currency.as_str(), "EUR");
    }
}
