//! Integration tests for the `SQLx` catalog repository.

use rustashop_domain::{CategoryRepository, ProductRepository};
use rustashop_persist_sqlx::{migrate, SqlxCatalogRepository};
use serenade_contracts::PageRequest;
use sqlx::postgres::{PgPool, PgPoolOptions};

const SCHEMA_LOCK: i64 = 874_512;
const HOODIE_PRODUCT: &str = "22222222-2222-2222-2222-222222222221";
const APPAREL: &str = "11111111-1111-1111-1111-111111111111";
const CHILD_CATEGORY: &str = "11111111-1111-1111-1111-111111111112";

async fn exclusive_catalog() -> Option<(PgPool, SqlxCatalogRepository)> {
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
        "INSERT INTO category (id, parent_id, slug, name) VALUES ($1::uuid, $2::uuid, 'tees', 'Tees')",
    )
    .bind(CHILD_CATEGORY)
    .bind(APPAREL)
    .execute(&pool)
    .await
    .expect("seed child category");
    sqlx::query(
        "INSERT INTO product (id, category_id, slug, name, enabled) VALUES
         ('22222222-2222-2222-2222-222222222221', '11111111-1111-1111-1111-111111111111', 'hoodie', 'Hoodie', TRUE),
         ('22222222-2222-2222-2222-222222222229', '11111111-1111-1111-1111-111111111111', 'retired', 'Retired', FALSE)",
    )
    .execute(&pool)
    .await
    .expect("seed products");
    sqlx::query(
        "INSERT INTO product_variant (id, product_id, sku, name, price_minor, currency, stock_quantity) VALUES
         ('33333333-3333-3333-3333-333333333331', '22222222-2222-2222-2222-222222222221', 'HOODIE-M', 'Medium', 4500, 'EUR', 8)",
    )
    .execute(&pool)
    .await
    .expect("seed variant");
    Some((pool.clone(), SqlxCatalogRepository::new(pool)))
}

async fn unlock(pool: &PgPool) {
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SCHEMA_LOCK)
        .execute(pool)
        .await
        .expect("unlock");
}

#[tokio::test]
async fn sqlx_catalog_lists_and_finds_seeded_rows() {
    let Some((pool, repo)) = exclusive_catalog().await else {
        return;
    };
    let product = ProductRepository::find_by_slug(&repo, "hoodie")
        .await
        .expect("slug")
        .expect("hoodie");
    assert_eq!(product.name, "Hoodie");
    let by_id = ProductRepository::find_by_id(&repo, &product.id)
        .await
        .expect("id")
        .expect("row");
    assert_eq!(by_id.slug, "hoodie");
    let bad_id = "not-a-uuid".to_owned();
    let missing_id = "00000000-0000-0000-0000-000000000000".to_owned();
    assert!(ProductRepository::find_by_id(&repo, &bad_id).await.is_err());
    assert!(ProductRepository::find_by_id(&repo, &missing_id)
        .await
        .expect("miss")
        .is_none());

    let listed = ProductRepository::list(&repo, PageRequest::first(10))
        .await
        .expect("list");
    assert_eq!(listed.len(), 1);
    let all = repo
        .list_all_products(PageRequest::first(10))
        .await
        .expect("list all");
    assert_eq!(all.len(), 2);

    let variants = repo
        .list_variants_for_product(HOODIE_PRODUCT)
        .await
        .expect("variants");
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].sku, "HOODIE-M");
    assert!(repo.list_variants_for_product("bad-id").await.is_err());
    unlock(&pool).await;
}

#[tokio::test]
async fn sqlx_catalog_category_parent_paths() {
    let Some((pool, repo)) = exclusive_catalog().await else {
        return;
    };
    let category = CategoryRepository::find_by_slug(&repo, "apparel", None)
        .await
        .expect("category slug")
        .expect("apparel");
    assert_eq!(category.name, "Apparel");
    let category_by_id = CategoryRepository::find_by_id(&repo, &category.id)
        .await
        .expect("category id")
        .expect("apparel by id");
    assert_eq!(category_by_id.slug, "apparel");
    let bad_id = "not-a-uuid".to_owned();
    assert!(CategoryRepository::find_by_id(&repo, &bad_id)
        .await
        .is_err());

    let apparel = APPAREL.to_owned();
    let child = CategoryRepository::find_by_slug(&repo, "tees", Some(&apparel))
        .await
        .expect("child slug")
        .expect("tees");
    assert_eq!(child.id, CHILD_CATEGORY);
    let under_parent =
        CategoryRepository::list_children(&repo, Some(&apparel), PageRequest::first(10))
            .await
            .expect("children under parent");
    assert_eq!(under_parent.len(), 1);
    assert_eq!(under_parent[0].slug, "tees");

    let children = CategoryRepository::list_children(&repo, None, PageRequest::first(10))
        .await
        .expect("children");
    assert_eq!(children.len(), 1);
    unlock(&pool).await;
}

#[tokio::test]
async fn list_variants_rejects_corrupt_currency() {
    let Some((pool, repo)) = exclusive_catalog().await else {
        return;
    };
    sqlx::query("UPDATE product_variant SET currency = 'ZZ' WHERE product_id = $1::uuid")
        .bind(HOODIE_PRODUCT)
        .execute(&pool)
        .await
        .expect("corrupt");
    let err = repo
        .list_variants_for_product(HOODIE_PRODUCT)
        .await
        .expect_err("bad currency");
    assert!(matches!(
        err,
        serenade_contracts::PersistenceError::Internal { .. }
    ));
    unlock(&pool).await;
}
