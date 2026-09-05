//! Integration tests for cart HTTP routes.

use actix_web::{test, web, App};
use rustashop_api::{commerce_http_kernel, routes, CartResponse};
use rustashop_persist::CatalogRepository;
use serde_json::json;

const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";
const SCHEMA_LOCK: i64 = 874_513;

#[actix_web::test]
async fn cart_crud_add_update_remove_and_totals() {
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
    assert_eq!(create_resp.status(), 201);
    let mut cart: CartResponse = test::read_body_json(create_resp).await;
    assert_eq!(cart.lines.len(), 0);
    assert_eq!(cart.items_total.amount_minor, 0);

    let get = test::TestRequest::get()
        .uri(&format!("/v1/carts/{}", cart.id))
        .to_request();
    let get_resp = test::call_service(&app, get).await;
    assert!(get_resp.status().is_success());
    let fetched: CartResponse = test::read_body_json(get_resp).await;
    assert_eq!(fetched.id, cart.id);

    let add = test::TestRequest::post()
        .uri(&format!("/v1/carts/{}/lines", cart.id))
        .set_json(json!({ "variant_id": HOODIE_VARIANT, "quantity": 2 }))
        .to_request();
    let add_resp = test::call_service(&app, add).await;
    assert!(add_resp.status().is_success());
    cart = test::read_body_json(add_resp).await;
    assert_eq!(cart.lines.len(), 1);
    assert_eq!(cart.lines[0].quantity, 2);
    assert_eq!(cart.lines[0].line_total.amount_minor, 9000);
    assert_eq!(cart.items_total.amount_minor, 9000);
    let line_id = cart.lines[0].id.clone();

    let merge = test::TestRequest::post()
        .uri(&format!("/v1/carts/{}/lines", cart.id))
        .set_json(json!({ "variant_id": HOODIE_VARIANT, "quantity": 1 }))
        .to_request();
    let merge_resp = test::call_service(&app, merge).await;
    assert!(merge_resp.status().is_success());
    cart = test::read_body_json(merge_resp).await;
    assert_eq!(cart.lines.len(), 1);
    assert_eq!(cart.lines[0].quantity, 3);
    assert_eq!(cart.items_total.amount_minor, 13_500);

    let patch = test::TestRequest::patch()
        .uri(&format!("/v1/carts/{}/lines/{line_id}", cart.id))
        .set_json(json!({ "quantity": 1 }))
        .to_request();
    let patch_resp = test::call_service(&app, patch).await;
    assert!(patch_resp.status().is_success());
    cart = test::read_body_json(patch_resp).await;
    assert_eq!(cart.lines[0].quantity, 1);
    assert_eq!(cart.items_total.amount_minor, 4500);

    let delete = test::TestRequest::delete()
        .uri(&format!("/v1/carts/{}/lines/{line_id}", cart.id))
        .to_request();
    let delete_resp = test::call_service(&app, delete).await;
    assert!(delete_resp.status().is_success());
    cart = test::read_body_json(delete_resp).await;
    assert_eq!(cart.lines.len(), 0);
    assert_eq!(cart.items_total.amount_minor, 0);

    let missing_variant = test::TestRequest::post()
        .uri(&format!("/v1/carts/{}/lines", cart.id))
        .set_json(json!({
            "variant_id": "00000000-0000-0000-0000-000000000000",
            "quantity": 1
        }))
        .to_request();
    let missing_resp = test::call_service(&app, missing_variant).await;
    assert_eq!(missing_resp.status(), 404);

    let bad_qty = test::TestRequest::post()
        .uri(&format!("/v1/carts/{}/lines", cart.id))
        .set_json(json!({ "variant_id": HOODIE_VARIANT, "quantity": 0 }))
        .to_request();
    let bad_resp = test::call_service(&app, bad_qty).await;
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
