//! Integration tests for the `SeaORM` catalog repository.

use rustashop_domain::{CategoryRepository, ProductRepository};
use rustashop_persist_seaorm::{migrate, SeaOrmCatalogRepository};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use serenade_contracts::{PageRequest, PersistenceError};

const SCHEMA_LOCK: i64 = 874_512;
const HOODIE_PRODUCT: &str = "22222222-2222-2222-2222-222222222221";
const APPAREL: &str = "11111111-1111-1111-1111-111111111111";
const CHILD_CATEGORY: &str = "11111111-1111-1111-1111-111111111112";

async fn exclusive_catalog() -> Option<(DatabaseConnection, SeaOrmCatalogRepository)> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return None;
    };
    let mut options = ConnectOptions::new(url);
    options.max_connections(1);
    let db = Database::connect(options).await.expect("connect");
    db.execute_unprepared(&format!("SELECT pg_advisory_lock({SCHEMA_LOCK})"))
        .await
        .expect("lock");
    db.execute_unprepared("DROP SCHEMA public CASCADE; CREATE SCHEMA public")
        .await
        .expect("reset schema");
    migrate(&db).await.expect("migrate");
    db.execute_unprepared(
        "INSERT INTO category (id, slug, name) VALUES ('11111111-1111-1111-1111-111111111111', 'apparel', 'Apparel')",
    )
    .await
    .expect("seed category");
    db.execute_unprepared(
        &format!(
            "INSERT INTO category (id, parent_id, slug, name) VALUES ('{CHILD_CATEGORY}', '{APPAREL}', 'tees', 'Tees')"
        ),
    )
    .await
    .expect("seed child category");
    db.execute_unprepared(
        "INSERT INTO product (id, category_id, slug, name, enabled) VALUES
         ('22222222-2222-2222-2222-222222222221', '11111111-1111-1111-1111-111111111111', 'hoodie', 'Hoodie', TRUE),
         ('22222222-2222-2222-2222-222222222229', '11111111-1111-1111-1111-111111111111', 'retired', 'Retired', FALSE)",
    )
    .await
    .expect("seed products");
    db.execute_unprepared(
        "INSERT INTO product_variant (id, product_id, sku, name, price_minor, currency, stock_quantity) VALUES
         ('33333333-3333-3333-3333-333333333331', '22222222-2222-2222-2222-222222222221', 'HOODIE-M', 'Medium', 4500, 'EUR', 8)",
    )
    .await
    .expect("seed variant");
    Some((db.clone(), SeaOrmCatalogRepository::new(db)))
}

async fn unlock(db: &DatabaseConnection) {
    db.execute_unprepared(&format!("SELECT pg_advisory_unlock({SCHEMA_LOCK})"))
        .await
        .expect("unlock");
}

#[tokio::test]
async fn seaorm_catalog_lists_and_finds_seeded_rows() {
    let Some((db, repo)) = exclusive_catalog().await else {
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
    assert!(matches!(
        repo.list_variants_for_product("bad-id").await,
        Err(PersistenceError::InvalidInput { .. })
    ));
    unlock(&db).await;
}

#[tokio::test]
async fn seaorm_catalog_category_parent_paths() {
    let Some((db, repo)) = exclusive_catalog().await else {
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

    let roots = CategoryRepository::list_children(&repo, None, PageRequest::first(10))
        .await
        .expect("roots");
    assert_eq!(roots.len(), 1);
    unlock(&db).await;
}

#[tokio::test]
async fn list_variants_rejects_corrupt_currency() {
    let Some((db, repo)) = exclusive_catalog().await else {
        return;
    };
    db.execute_unprepared(&format!(
        "UPDATE product_variant SET currency = 'ZZ' WHERE product_id = '{HOODIE_PRODUCT}'"
    ))
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
    unlock(&db).await;
}
