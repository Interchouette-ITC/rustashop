//! Schema-break fault injection for `PersistenceError::Internal` arms (`SQLx`).

use rustashop_domain::{CategoryRepository, Currency, ProductRepository};
use rustashop_persist_sqlx::{SqlxCatalogRepository, migrate, seed_catalog};
use serenade_contracts::{PageRequest, PersistenceError};
use sqlx::postgres::{PgPool, PgPoolOptions};

const SCHEMA_LOCK: i64 = 874_530;
const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";

async fn exclusive_repo() -> Option<(PgPool, SqlxCatalogRepository)> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return None;
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SCHEMA_LOCK)
        .execute(&pool)
        .await
        .expect("lock");
    reset_schema(&pool).await;
    Some((pool.clone(), SqlxCatalogRepository::new(pool)))
}

async fn reset_schema(pool: &PgPool) {
    sqlx::query("DROP SCHEMA public CASCADE")
        .execute(pool)
        .await
        .expect("drop");
    sqlx::query("CREATE SCHEMA public")
        .execute(pool)
        .await
        .expect("create");
    migrate(pool).await.expect("migrate");
    seed_catalog(pool).await.expect("seed");
}

async fn unlock(pool: &PgPool) {
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SCHEMA_LOCK)
        .execute(pool)
        .await
        .expect("unlock");
}

fn assert_internal(err: &PersistenceError) {
    assert!(
        matches!(err, PersistenceError::Internal { .. }),
        "expected Internal, got {err:?}"
    );
}

async fn seed_cart_with_hoodie(repo: &SqlxCatalogRepository) -> rustashop_domain::Cart {
    let currency = Currency::new("EUR").expect("EUR");
    let cart = repo.create_cart(&currency).await.expect("create");
    let (variant, product_name) = repo
        .find_variant_for_cart(HOODIE_VARIANT)
        .await
        .expect("variant")
        .expect("hoodie");
    let mut loaded = repo
        .find_cart_by_id(&cart.id)
        .await
        .expect("load")
        .expect("cart");
    loaded.lines.push(rustashop_domain::CartLine {
        id: String::new(),
        cart_id: loaded.id.clone(),
        variant_id: variant.id,
        quantity: 1,
        unit_price: variant.price,
        product_name,
        variant_sku: variant.sku,
    });
    repo.save_cart(&loaded).await.expect("save");
    loaded
}

async fn drop_commerce_tables(pool: &PgPool) {
    for sql in [
        "DROP TABLE cart_line CASCADE",
        "DROP TABLE cart CASCADE",
        r#"DROP TABLE "order" CASCADE"#,
        "DROP TABLE product_variant CASCADE",
        "DROP TABLE product CASCADE",
        "DROP TABLE category CASCADE",
    ] {
        sqlx::query(sql).execute(pool).await.expect("drop");
    }
}

async fn assert_reads_are_internal(repo: &SqlxCatalogRepository, cart: &rustashop_domain::Cart) {
    let currency = Currency::new("EUR").expect("EUR");
    assert_internal(&repo.find_cart_by_id(&cart.id).await.expect_err("find cart"));
    assert_internal(
        &rustashop_domain::CartRepository::find_by_token(repo, &cart.token)
            .await
            .expect_err("token"),
    );
    assert_internal(&repo.create_cart(&currency).await.expect_err("create"));
    assert_internal(
        &repo
            .checkout_cart(&cart.id, Some("sqlx-internal-key"))
            .await
            .expect_err("checkout"),
    );
    assert_internal(
        &repo
            .find_variant_for_cart(HOODIE_VARIANT)
            .await
            .expect_err("variant"),
    );
    assert_internal(
        &ProductRepository::list(repo, PageRequest::first(10))
            .await
            .expect_err("products"),
    );
    assert_internal(
        &CategoryRepository::list_children(repo, None, PageRequest::first(10))
            .await
            .expect_err("categories"),
    );
    assert_internal(
        &repo
            .get_order("00000000-0000-0000-0000-000000000001")
            .await
            .expect_err("order"),
    );
    assert_internal(
        &repo
            .list_all_products(PageRequest::first(5))
            .await
            .expect_err("admin products"),
    );
    assert_internal(
        &repo
            .list_variants_for_product("22222222-2222-2222-2222-222222222221")
            .await
            .expect_err("variants"),
    );
}

#[tokio::test]
async fn dropped_tables_surface_internal_on_reads_and_checkout() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    let cart = seed_cart_with_hoodie(&repo).await;
    drop_commerce_tables(&pool).await;
    assert_reads_are_internal(&repo, &cart).await;
    reset_schema(&pool).await;
    unlock(&pool).await;
}

#[tokio::test]
async fn closed_pool_surfaces_internal_on_begin() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    let currency = Currency::new("EUR").expect("EUR");
    let cart = repo.create_cart(&currency).await.expect("create");
    unlock(&pool).await;
    pool.close().await;
    assert_internal(
        &repo
            .checkout_cart(&cart.id, None)
            .await
            .expect_err("closed pool checkout"),
    );

    let Ok(url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let fresh = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("reconnect");
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SCHEMA_LOCK)
        .execute(&fresh)
        .await
        .expect("relock");
    reset_schema(&fresh).await;
    unlock(&fresh).await;
}

async fn expect_checkout_internal(repo: &SqlxCatalogRepository, cart_id: &str, label: &str) {
    let err = repo.checkout_cart(cart_id, None).await.expect_err(label);
    assert_internal(&err);
}

async fn install_reject_cart_update_trigger(pool: &PgPool) {
    sqlx::query(
        r"
CREATE OR REPLACE FUNCTION rustashop_reject_cart_update() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'fault: cart update blocked';
END;
$$ LANGUAGE plpgsql;
",
    )
    .execute(pool)
    .await
    .expect("fn");
    sqlx::query(
        r"
CREATE TRIGGER rustashop_reject_cart_update
  BEFORE UPDATE ON cart
  FOR EACH ROW EXECUTE FUNCTION rustashop_reject_cart_update()
",
    )
    .execute(pool)
    .await
    .expect("trigger");
}

async fn install_fail_commit_trigger(pool: &PgPool) {
    sqlx::query(
        r"
CREATE OR REPLACE FUNCTION rustashop_fail_commit() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'fault: deferred commit blocked';
END;
$$ LANGUAGE plpgsql;
",
    )
    .execute(pool)
    .await
    .expect("fn commit");
    sqlx::query(
        r"
CREATE CONSTRAINT TRIGGER rustashop_fail_commit
  AFTER UPDATE ON cart
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW
  EXECUTE FUNCTION rustashop_fail_commit()
",
    )
    .execute(pool)
    .await
    .expect("constraint trigger");
}

#[tokio::test]
async fn staged_checkout_faults_hit_deeper_internal_arms() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };

    let cart = seed_cart_with_hoodie(&repo).await;
    sqlx::query("DROP TABLE cart_line CASCADE")
        .execute(&pool)
        .await
        .expect("drop cart_line");
    expect_checkout_internal(&repo, &cart.id, "cart_line gone").await;
    reset_schema(&pool).await;

    let cart = seed_cart_with_hoodie(&repo).await;
    sqlx::query(r#"DROP TABLE "order" CASCADE"#)
        .execute(&pool)
        .await
        .expect("drop order");
    expect_checkout_internal(&repo, &cart.id, "order gone").await;
    assert_internal(
        &repo
            .list_orders(PageRequest::first(5))
            .await
            .expect_err("list orders"),
    );
    assert_internal(
        &repo
            .update_order_state(
                "00000000-0000-0000-0000-000000000001",
                rustashop_domain::OrderState::Paid,
            )
            .await
            .expect_err("update order"),
    );
    reset_schema(&pool).await;

    let cart = seed_cart_with_hoodie(&repo).await;
    sqlx::query("DROP TABLE order_line CASCADE")
        .execute(&pool)
        .await
        .expect("drop order_line");
    expect_checkout_internal(&repo, &cart.id, "order_line gone").await;
    reset_schema(&pool).await;

    let cart = seed_cart_with_hoodie(&repo).await;
    install_reject_cart_update_trigger(&pool).await;
    expect_checkout_internal(&repo, &cart.id, "cart update blocked").await;
    reset_schema(&pool).await;

    let cart = seed_cart_with_hoodie(&repo).await;
    install_fail_commit_trigger(&pool).await;
    expect_checkout_internal(&repo, &cart.id, "deferred commit").await;
    reset_schema(&pool).await;

    unlock(&pool).await;
}
