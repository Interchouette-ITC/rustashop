//! Integration tests for catalog product HTTP routes.

use actix_web::{test, web, App};
use rustashop_api::{commerce_http_kernel, routes, ProductDetailResponse, ProductListResponse};
use rustashop_persist::CatalogRepository;

const HOODIE_ID: &str = "22222222-2222-2222-2222-222222222221";
const SCHEMA_LOCK: i64 = 874_512;

#[actix_web::test]
async fn products_list_and_get_seeded_rows() {
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

    let list_req = test::TestRequest::get().uri("/v1/products").to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert!(list_resp.status().is_success());
    let mut list: ProductListResponse = test::read_body_json(list_resp).await;
    assert_eq!(list.items.len(), 3);
    assert!(list.items.iter().any(|item| item.slug == "hoodie"));
    list.items.sort_by(|left, right| left.slug.cmp(&right.slug));
    insta::assert_json_snapshot!("products_list_seeded", list);

    let get_req = test::TestRequest::get()
        .uri(&format!("/v1/products/{HOODIE_ID}"))
        .to_request();
    let get_resp = test::call_service(&app, get_req).await;
    assert!(get_resp.status().is_success());
    let hoodie: ProductDetailResponse = test::read_body_json(get_resp).await;
    assert_eq!(hoodie.slug, "hoodie");
    assert_eq!(hoodie.name, "Hoodie");
    assert!(hoodie
        .variants
        .iter()
        .any(|variant| variant.sku.contains("HOODIE")));

    let missing = test::TestRequest::get()
        .uri("/v1/products/00000000-0000-0000-0000-000000000000")
        .to_request();
    let missing_resp = test::call_service(&app, missing).await;
    assert_eq!(missing_resp.status(), 404);

    let paged = test::TestRequest::get()
        .uri("/v1/products?limit=1&offset=0")
        .to_request();
    let paged_resp = test::call_service(&app, paged).await;
    assert!(paged_resp.status().is_success());
    let page: ProductListResponse = test::read_body_json(paged_resp).await;
    assert_eq!(page.items.len(), 1);
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
