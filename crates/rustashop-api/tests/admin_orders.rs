//! Integration tests for admin order HTTP (#19).

use actix_web::{test, web, App};
use rustashop_api::{
    commerce_http_kernel, routes, AdminAuthConfig, CartResponse, OrderListResponse, OrderResponse,
};
use rustashop_persist::CatalogRepository;
use serde_json::json;

const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";
const SCHEMA_LOCK: i64 = 874_519;
const ADMIN_TOKEN: &str = "test-admin-token";

#[actix_web::test]
async fn admin_orders_require_bearer_and_can_mark_shipped() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let catalog = exclusive_seeded_catalog().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(commerce_http_kernel(Some(catalog.clone()))))
            .app_data(web::Data::new(catalog))
            .app_data(web::Data::new(AdminAuthConfig::from_token(ADMIN_TOKEN)))
            .configure(routes),
    )
    .await;

    let denied = test::TestRequest::get()
        .uri("/v1/admin/orders")
        .to_request();
    let denied_resp = test::call_service(&app, denied).await;
    assert_eq!(denied_resp.status(), 401);

    let wrong = test::TestRequest::get()
        .uri("/v1/admin/orders")
        .insert_header(("Authorization", "Bearer wrong-token"))
        .to_request();
    let wrong_resp = test::call_service(&app, wrong).await;
    assert_eq!(wrong_resp.status(), 401);

    let create = test::TestRequest::post()
        .uri("/v1/carts")
        .set_json(json!({ "currency": "EUR" }))
        .to_request();
    let create_resp = test::call_service(&app, create).await;
    let cart: CartResponse = test::read_body_json(create_resp).await;

    let add = test::TestRequest::post()
        .uri(&format!("/v1/carts/{}/lines", cart.id))
        .set_json(json!({ "variant_id": HOODIE_VARIANT, "quantity": 1 }))
        .to_request();
    assert!(test::call_service(&app, add).await.status().is_success());

    let checkout = test::TestRequest::post()
        .uri("/v1/checkout")
        .set_json(json!({ "cart_id": cart.id }))
        .to_request();
    let checkout_resp = test::call_service(&app, checkout).await;
    assert_eq!(checkout_resp.status(), 201);
    let placed: OrderResponse = test::read_body_json(checkout_resp).await;
    assert_eq!(placed.state, "placed");

    let list = test::TestRequest::get()
        .uri("/v1/admin/orders")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .to_request();
    let list_resp = test::call_service(&app, list).await;
    assert!(list_resp.status().is_success());
    let page: OrderListResponse = test::read_body_json(list_resp).await;
    assert!(page.items.iter().any(|order| order.id == placed.id));

    let patch = test::TestRequest::patch()
        .uri(&format!("/v1/admin/orders/{}", placed.id))
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .set_json(json!({ "status": "shipped" }))
        .to_request();
    let patch_resp = test::call_service(&app, patch).await;
    assert!(patch_resp.status().is_success());
    let shipped: OrderResponse = test::read_body_json(patch_resp).await;
    assert_eq!(shipped.id, placed.id);
    assert_eq!(shipped.state, "shipped");

    let bad_status = test::TestRequest::patch()
        .uri(&format!("/v1/admin/orders/{}", placed.id))
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .set_json(json!({ "status": "draft" }))
        .to_request();
    let bad_resp = test::call_service(&app, bad_status).await;
    assert_eq!(bad_resp.status(), 422);
}

#[cfg(feature = "persist-sqlx")]
async fn exclusive_seeded_catalog() -> CatalogRepository {
    use rustashop_persist_sqlx::{migrate, seed_catalog, SqlxCatalogRepository};
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
    SqlxCatalogRepository::new(pool)
}

#[cfg(feature = "persist-seaorm")]
async fn exclusive_seeded_catalog() -> CatalogRepository {
    use rustashop_persist_seaorm::{migrate, seed_catalog, SeaOrmCatalogRepository};
    use sea_orm::{ConnectOptions, ConnectionTrait, Database};

    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let mut options = ConnectOptions::new(url);
    options.max_connections(1);
    let db = Database::connect(options).await.expect("connect");
    db.execute_unprepared(&format!("SELECT pg_advisory_lock({SCHEMA_LOCK})"))
        .await
        .expect("lock");
    db.execute_unprepared("DROP SCHEMA public CASCADE; CREATE SCHEMA public")
        .await
        .expect("reset");
    migrate(&db).await.expect("migrate");
    seed_catalog(&db).await.expect("seed");
    SeaOrmCatalogRepository::new(db)
}
