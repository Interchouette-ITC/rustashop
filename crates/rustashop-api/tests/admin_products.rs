//! Integration tests for admin product HTTP (#102).

use actix_web::{test, web, App};
use rustashop_api::{commerce_http_kernel, routes, AdminAuthConfig, ProductListResponse};
use rustashop_persist::CatalogRepository;

const SCHEMA_LOCK: i64 = 874_520;
const ADMIN_TOKEN: &str = "test-admin-token";

#[actix_web::test]
async fn admin_products_require_bearer_and_list() {
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
        .uri("/v1/admin/products")
        .to_request();
    let denied_resp = test::call_service(&app, denied).await;
    assert_eq!(denied_resp.status(), 401);

    let list = test::TestRequest::get()
        .uri("/v1/admin/products")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .to_request();
    let list_resp = test::call_service(&app, list).await;
    assert!(list_resp.status().is_success());
    let page: ProductListResponse = test::read_body_json(list_resp).await;
    assert!(
        page.items.iter().any(|product| product.slug == "hoodie"),
        "expected seeded hoodie in admin product list"
    );
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
