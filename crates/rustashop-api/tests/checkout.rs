//! Integration tests for checkout HTTP.

use actix_web::{test, web, App};
use rustashop_api::{commerce_http_kernel, routes, CartResponse, OrderResponse};
use rustashop_persist::CatalogRepository;
use serde_json::json;

const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";
const SCHEMA_LOCK: i64 = 874_514;

#[actix_web::test]
async fn checkout_places_order_and_replays_idempotency_key() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let catalog = exclusive_seeded_catalog().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(commerce_http_kernel(Some(catalog.clone()))))
            .app_data(web::Data::new(catalog))
            .configure(routes),
    )
    .await;

    let create = test::TestRequest::post()
        .uri("/v1/carts")
        .set_json(json!({ "currency": "EUR" }))
        .to_request();
    let create_resp = test::call_service(&app, create).await;
    let cart: CartResponse = test::read_body_json(create_resp).await;

    let empty = test::TestRequest::post()
        .uri("/v1/checkout")
        .set_json(json!({ "cart_id": cart.id }))
        .to_request();
    let empty_resp = test::call_service(&app, empty).await;
    assert_eq!(empty_resp.status(), 422);

    let add = test::TestRequest::post()
        .uri(&format!("/v1/carts/{}/lines", cart.id))
        .set_json(json!({ "variant_id": HOODIE_VARIANT, "quantity": 2 }))
        .to_request();
    let add_resp = test::call_service(&app, add).await;
    assert!(add_resp.status().is_success());

    let checkout = test::TestRequest::post()
        .uri("/v1/checkout")
        .insert_header(("Idempotency-Key", "checkout-key-1"))
        .set_json(json!({ "cart_id": cart.id }))
        .to_request();
    let checkout_resp = test::call_service(&app, checkout).await;
    assert_eq!(checkout_resp.status(), 201);
    let order: OrderResponse = test::read_body_json(checkout_resp).await;
    assert_eq!(order.payment_status, "pending");
    assert_eq!(order.state, "placed");
    assert_eq!(order.total.amount_minor, 9000);
    assert_eq!(order.lines.len(), 1);
    let order_id = order.id.clone();

    let replay = test::TestRequest::post()
        .uri("/v1/checkout")
        .insert_header(("Idempotency-Key", "checkout-key-1"))
        .set_json(json!({ "cart_id": cart.id }))
        .to_request();
    let replay_resp = test::call_service(&app, replay).await;
    assert_eq!(replay_resp.status(), 201);
    let replayed: OrderResponse = test::read_body_json(replay_resp).await;
    assert_eq!(replayed.id, order_id);

    let again = test::TestRequest::post()
        .uri("/v1/checkout")
        .set_json(json!({ "cart_id": cart.id }))
        .to_request();
    let again_resp = test::call_service(&app, again).await;
    assert_eq!(again_resp.status(), 409);
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
