//! Schema-break fault injection for `PersistenceError::Internal` arms (`SeaORM`).

use rustashop_domain::{CategoryRepository, Currency, ProductRepository};
use rustashop_persist_seaorm::{SeaOrmCatalogRepository, migrate, seed_catalog};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use serenade_contracts::{PageRequest, PersistenceError};

/// Same lock key as the `SQLx` fault suite so both adapters serialize on one Postgres.
const SCHEMA_LOCK: i64 = 874_530;
const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";

async fn exclusive_repo() -> Option<(DatabaseConnection, SeaOrmCatalogRepository)> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return None;
    };
    let mut options = ConnectOptions::new(url);
    options.max_connections(2);
    let db = Database::connect(options).await.expect("connect");
    db.execute_unprepared(&format!("SELECT pg_advisory_lock({SCHEMA_LOCK})"))
        .await
        .expect("lock");
    reset_schema(&db).await;
    Some((db.clone(), SeaOrmCatalogRepository::new(db)))
}

async fn reset_schema(db: &DatabaseConnection) {
    db.execute_unprepared("DROP SCHEMA public CASCADE; CREATE SCHEMA public")
        .await
        .expect("reset");
    migrate(db).await.expect("migrate");
    seed_catalog(db).await.expect("seed");
}

async fn unlock(db: &DatabaseConnection) {
    db.execute_unprepared(&format!("SELECT pg_advisory_unlock({SCHEMA_LOCK})"))
        .await
        .expect("unlock");
}

fn assert_internal(err: &PersistenceError) {
    assert!(
        matches!(err, PersistenceError::Internal { .. }),
        "expected Internal, got {err:?}"
    );
}

async fn seed_cart_with_hoodie(repo: &SeaOrmCatalogRepository) -> rustashop_domain::Cart {
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

async fn drop_commerce_tables(db: &DatabaseConnection) {
    db.execute_unprepared(
        r#"
DROP TABLE IF EXISTS order_line CASCADE;
DROP TABLE IF EXISTS "order" CASCADE;
DROP TABLE IF EXISTS cart_line CASCADE;
DROP TABLE IF EXISTS cart CASCADE;
DROP TABLE IF EXISTS product_variant CASCADE;
DROP TABLE IF EXISTS product CASCADE;
DROP TABLE IF EXISTS category CASCADE;
"#,
    )
    .await
    .expect("drop tables");
}

async fn assert_reads_are_internal(repo: &SeaOrmCatalogRepository, cart: &rustashop_domain::Cart) {
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
            .checkout_cart(&cart.id, Some("seaorm-internal-key"))
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
    let Some((db, repo)) = exclusive_repo().await else {
        return;
    };
    let cart = seed_cart_with_hoodie(&repo).await;
    drop_commerce_tables(&db).await;
    assert_reads_are_internal(&repo, &cart).await;
    reset_schema(&db).await;
    unlock(&db).await;
}

async fn expect_checkout_internal(repo: &SeaOrmCatalogRepository, cart_id: &str, label: &str) {
    let err = repo.checkout_cart(cart_id, None).await.expect_err(label);
    assert_internal(&err);
}

async fn install_reject_cart_update_trigger(db: &DatabaseConnection) {
    db.execute_unprepared(
        r"
CREATE OR REPLACE FUNCTION rustashop_reject_cart_update() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'fault: cart update blocked';
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER rustashop_reject_cart_update
  BEFORE UPDATE ON cart
  FOR EACH ROW EXECUTE FUNCTION rustashop_reject_cart_update();
",
    )
    .await
    .expect("trigger");
}

async fn install_fail_commit_trigger(db: &DatabaseConnection) {
    db.execute_unprepared(
        r"
CREATE OR REPLACE FUNCTION rustashop_fail_commit() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'fault: deferred commit blocked';
END;
$$ LANGUAGE plpgsql;
CREATE CONSTRAINT TRIGGER rustashop_fail_commit
  AFTER UPDATE ON cart
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW
  EXECUTE FUNCTION rustashop_fail_commit();
",
    )
    .await
    .expect("constraint trigger");
}

#[tokio::test]
async fn staged_checkout_faults_hit_deeper_internal_arms() {
    let Some((db, repo)) = exclusive_repo().await else {
        return;
    };

    let cart = seed_cart_with_hoodie(&repo).await;
    db.execute_unprepared("DROP TABLE cart_line CASCADE")
        .await
        .expect("drop cart_line");
    expect_checkout_internal(&repo, &cart.id, "cart_line gone").await;
    assert_internal(
        &repo
            .save_cart(&cart)
            .await
            .expect_err("save without cart_line"),
    );
    reset_schema(&db).await;

    let cart = seed_cart_with_hoodie(&repo).await;
    db.execute_unprepared(r#"DROP TABLE "order" CASCADE"#)
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
    reset_schema(&db).await;

    let cart = seed_cart_with_hoodie(&repo).await;
    db.execute_unprepared("DROP TABLE order_line CASCADE")
        .await
        .expect("drop order_line");
    expect_checkout_internal(&repo, &cart.id, "order_line gone").await;
    reset_schema(&db).await;

    let cart = seed_cart_with_hoodie(&repo).await;
    install_reject_cart_update_trigger(&db).await;
    expect_checkout_internal(&repo, &cart.id, "cart update blocked").await;
    reset_schema(&db).await;

    let cart = seed_cart_with_hoodie(&repo).await;
    install_fail_commit_trigger(&db).await;
    expect_checkout_internal(&repo, &cart.id, "deferred commit").await;
    assert_internal(&repo.save_cart(&cart).await.expect_err("save commit"));
    reset_schema(&db).await;

    unlock(&db).await;
}

#[tokio::test]
async fn cart_load_lines_delete_and_checkout_replay_internals() {
    let Some((db, repo)) = exclusive_repo().await else {
        return;
    };

    // load_lines Internal: cart header present, cart_line table gone.
    let cart = seed_cart_with_hoodie(&repo).await;
    db.execute_unprepared("DROP TABLE cart_line CASCADE")
        .await
        .expect("drop cart_line");
    assert_internal(
        &repo
            .find_cart_by_id(&cart.id)
            .await
            .expect_err("find load_lines"),
    );
    assert_internal(
        &rustashop_domain::CartRepository::find_by_token(&repo, &cart.token)
            .await
            .expect_err("token load_lines"),
    );
    reset_schema(&db).await;

    // delete Internal when cart table is gone.
    let cart = seed_cart_with_hoodie(&repo).await;
    db.execute_unprepared("DROP TABLE cart CASCADE")
        .await
        .expect("drop cart");
    assert_internal(
        &rustashop_domain::CartRepository::delete(&repo, &cart.id)
            .await
            .expect_err("delete"),
    );
    reset_schema(&db).await;

    // product lookup Internal after variant row still exists.
    let cart = seed_cart_with_hoodie(&repo).await;
    let _ = cart;
    db.execute_unprepared("ALTER TABLE product DROP COLUMN name")
        .await
        .expect("break product");
    assert_internal(
        &repo
            .find_variant_for_cart(HOODIE_VARIANT)
            .await
            .expect_err("product broken"),
    );
    reset_schema(&db).await;

    // save_cart line insert Internal (delete_many still works).
    let cart = seed_cart_with_hoodie(&repo).await;
    db.execute_unprepared(
        r"
CREATE OR REPLACE FUNCTION rustashop_reject_line_insert() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'fault: cart_line insert blocked';
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER rustashop_reject_line_insert
  BEFORE INSERT ON cart_line
  FOR EACH ROW EXECUTE FUNCTION rustashop_reject_line_insert();
",
    )
    .await
    .expect("line insert trigger");
    assert_internal(&repo.save_cart(&cart).await.expect_err("line insert"));
    reset_schema(&db).await;

    // save_cart header update Internal.
    let cart = seed_cart_with_hoodie(&repo).await;
    install_reject_cart_update_trigger(&db).await;
    assert_internal(&repo.save_cart(&cart).await.expect_err("cart update"));
    reset_schema(&db).await;

    // find_order_by_key Internal after a real order exists (lines table dropped).
    let cart = seed_cart_with_hoodie(&repo).await;
    let key = "seaorm-replay-key";
    let order = repo
        .checkout_cart(&cart.id, Some(key))
        .await
        .expect("checkout");
    assert_eq!(order.idempotency_key.as_deref(), Some(key));
    db.execute_unprepared("DROP TABLE order_line CASCADE")
        .await
        .expect("drop order_line");
    assert_internal(
        &repo
            .checkout_cart(&cart.id, Some(key))
            .await
            .expect_err("replay lines gone"),
    );
    reset_schema(&db).await;

    // Checked-out cart + unused key: cart_status conflict path inside txn.
    let cart = seed_cart_with_hoodie(&repo).await;
    repo.checkout_cart(&cart.id, None)
        .await
        .expect("first checkout");
    let err = repo
        .checkout_cart(&cart.id, Some("unused-after-checkout"))
        .await
        .expect_err("checked out");
    assert!(matches!(
        err,
        PersistenceError::Conflict {
            constraint: "cart_status"
        }
    ));

    unlock(&db).await;
}

#[tokio::test]
async fn closed_db_surfaces_internal_on_begin() {
    let Some((db, repo)) = exclusive_repo().await else {
        return;
    };
    let currency = Currency::new("EUR").expect("EUR");
    let cart = repo.create_cart(&currency).await.expect("create");
    unlock(&db).await;
    db.close_by_ref().await.expect("close");
    assert_internal(
        &repo
            .checkout_cart(&cart.id, None)
            .await
            .expect_err("closed db checkout"),
    );
    assert_internal(&repo.save_cart(&cart).await.expect_err("closed db save"));

    let Ok(url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let mut options = ConnectOptions::new(url);
    options.max_connections(1);
    let fresh = Database::connect(options).await.expect("reconnect");
    fresh
        .execute_unprepared(&format!("SELECT pg_advisory_lock({SCHEMA_LOCK})"))
        .await
        .expect("relock");
    reset_schema(&fresh).await;
    unlock(&fresh).await;
}
