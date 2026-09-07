//! Integration test for Diesel `find_by_id` against a migrated schema.

use rustashop_domain::ProductRepository;
use rustashop_persist_diesel::DieselCatalogRepository;
use rustashop_persist_sqlx::migrate;
use sqlx::postgres::{PgPool, PgPoolOptions};

const SCHEMA_LOCK: i64 = 874_513;
const HOODIE_PRODUCT: &str = "22222222-2222-2222-2222-222222222221";

async fn exclusive_repo() -> Option<(PgPool, DieselCatalogRepository)> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return None;
    };
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
        .expect("drop schema");
    sqlx::query("CREATE SCHEMA public")
        .execute(&pool)
        .await
        .expect("create schema");
    migrate(&pool).await.expect("migrate");
    sqlx::query(
        "INSERT INTO category (id, slug, name) VALUES ('11111111-1111-1111-1111-111111111111', 'apparel', 'Apparel')",
    )
    .execute(&pool)
    .await
    .expect("seed category");
    sqlx::query(
        "INSERT INTO product (id, category_id, slug, name, enabled) VALUES
         ('22222222-2222-2222-2222-222222222221', '11111111-1111-1111-1111-111111111111', 'hoodie', 'Hoodie', TRUE)",
    )
    .execute(&pool)
    .await
    .expect("seed product");

    let repo = DieselCatalogRepository::connect(&url)
        .await
        .expect("diesel connect");
    Some((pool, repo))
}

async fn unlock(pool: &PgPool) {
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SCHEMA_LOCK)
        .execute(pool)
        .await
        .expect("unlock");
}

#[tokio::test]
async fn diesel_find_by_id_loads_seeded_product() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    let found = repo
        .find_by_id(&HOODIE_PRODUCT.to_owned())
        .await
        .expect("find_by_id");
    let product = found.expect("hoodie");
    assert_eq!(product.slug, "hoodie");
    assert_eq!(product.name, "Hoodie");
    assert!(product.enabled);

    let missing = repo
        .find_by_id(&"00000000-0000-0000-0000-000000000000".to_owned())
        .await
        .expect("missing id");
    assert!(missing.is_none());

    unlock(&pool).await;
}
