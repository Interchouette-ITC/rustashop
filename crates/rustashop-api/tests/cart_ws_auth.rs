//! Cart WebSocket auth fails before the upgrade when token/cart is invalid.

#![cfg(feature = "persist-sqlx")]

use actix_web::{test, web};
use rustashop_api::{
    AdminAuthConfig, CartHub, CartResponse, CommerceFrontConfig, CommerceListenData,
    DEFAULT_ADMIN_API_PREFIX, SandboxJobHub, commerce_app, commerce_http_kernel,
};
use rustashop_persist::CatalogRepository;
use serde_json::json;

const SCHEMA_LOCK: i64 = 874_515;

fn listen_data(
    kernel: serenade_http::AsyncHttpKernel,
    hub: CartHub,
    catalog: CatalogRepository,
) -> CommerceListenData {
    CommerceListenData {
        kernel: web::Data::new(kernel),
        cart_hub: web::Data::new(hub),
        catalog: web::Data::new(catalog),
        sandbox_hub: web::Data::new(SandboxJobHub::new()),
        admin_auth: web::Data::new(AdminAuthConfig::from_token("")),
        admin_prefix: DEFAULT_ADMIN_API_PREFIX.to_owned(),
    }
}

#[actix_web::test]
async fn cart_ws_rejects_empty_token_and_unknown_cart() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let catalog = exclusive_seeded_catalog().await;
    let hub = CartHub::new();
    let kernel = commerce_http_kernel(CommerceFrontConfig {
        catalog: Some(catalog.clone()),
        cart_hub: Some(hub.clone()),
        ..CommerceFrontConfig::test_default()
    });
    let app = test::init_service(commerce_app(listen_data(kernel, hub, catalog))).await;

    let empty = test::TestRequest::get()
        .uri("/v1/carts/11111111-1111-1111-1111-111111111111/ws?token=")
        .to_request();
    let empty_resp = test::call_service(&app, empty).await;
    assert_eq!(empty_resp.status(), 401);

    let missing = test::TestRequest::get()
        .uri("/v1/carts/11111111-1111-1111-1111-111111111111/ws?token=nope")
        .to_request();
    let missing_resp = test::call_service(&app, missing).await;
    assert_eq!(missing_resp.status(), 404);
}

#[actix_web::test]
async fn cart_ws_rejects_wrong_token() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let catalog = exclusive_seeded_catalog().await;
    let hub = CartHub::new();
    let config = CommerceFrontConfig {
        catalog: Some(catalog.clone()),
        cart_hub: Some(hub.clone()),
        ..CommerceFrontConfig::test_default()
    };
    let http_app = test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(commerce_http_kernel(config.clone())))
            .configure(rustashop_api::routes),
    )
    .await;

    let create = test::TestRequest::post()
        .uri("/v1/carts")
        .set_json(json!({ "currency": "EUR" }))
        .to_request();
    let create_resp = test::call_service(&http_app, create).await;
    assert_eq!(create_resp.status(), 201);
    let cart: CartResponse = test::read_body_json(create_resp).await;

    let ws_app = test::init_service(commerce_app(listen_data(
        commerce_http_kernel(config),
        hub,
        catalog,
    )))
    .await;
    let bad = test::TestRequest::get()
        .uri(&format!("/v1/carts/{}/ws?token=wrong", cart.id))
        .to_request();
    let bad_resp = test::call_service(&ws_app, bad).await;
    assert_eq!(bad_resp.status(), 403);
}

#[actix_web::test]
async fn cart_ws_returns_500_when_catalog_errors() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let (catalog, pool) = exclusive_seeded_catalog_with_pool().await;
    let hub = CartHub::new();
    let kernel = commerce_http_kernel(CommerceFrontConfig {
        catalog: Some(catalog.clone()),
        cart_hub: Some(hub.clone()),
        ..CommerceFrontConfig::test_default()
    });
    pool.close().await;
    let app = test::init_service(commerce_app(listen_data(kernel, hub, catalog))).await;
    let req = test::TestRequest::get()
        .uri("/v1/carts/11111111-1111-1111-1111-111111111111/ws?token=x")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

async fn exclusive_seeded_catalog() -> CatalogRepository {
    exclusive_seeded_catalog_with_pool().await.0
}

async fn exclusive_seeded_catalog_with_pool() -> (CatalogRepository, sqlx::PgPool) {
    use rustashop_persist_sqlx::{SqlxCatalogRepository, migrate, seed_catalog};
    use sqlx::postgres::PgPoolOptions;

    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("connect");
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SCHEMA_LOCK)
        .execute(&pool)
        .await
        .expect("lock");
    sqlx::query("DROP SCHEMA public CASCADE")
        .execute(&pool)
        .await
        .expect("drop");
    sqlx::query("CREATE SCHEMA public")
        .execute(&pool)
        .await
        .expect("create");
    migrate(&pool).await.expect("migrate");
    seed_catalog(&pool).await.expect("seed");
    let catalog = SqlxCatalogRepository::new(pool.clone());
    (catalog, pool)
}
