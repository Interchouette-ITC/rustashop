//! Cart and checkout repository edge coverage for `SQLx`.

use rustashop_domain::{CartLine, CartRepository, CartStatus, Currency, Money};
use rustashop_persist_sqlx::{migrate, seed_catalog, SqlxCatalogRepository};
use serenade_contracts::PersistenceError;
use sqlx::postgres::PgPool;
use sqlx::postgres::PgPoolOptions;

const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";
const MUG_VARIANT: &str = "33333333-3333-3333-3333-333333333332";
const SCHEMA_LOCK: i64 = 874_520;
const CUSTOMER_ID: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

async fn exclusive_repo() -> Option<(PgPool, SqlxCatalogRepository)> {
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
        .expect("drop");
    sqlx::query("CREATE SCHEMA public")
        .execute(&pool)
        .await
        .expect("create");
    migrate(&pool).await.expect("migrate");
    seed_catalog(&pool).await.expect("seed");
    Some((pool.clone(), SqlxCatalogRepository::new(pool)))
}

async fn unlock(pool: &PgPool) {
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SCHEMA_LOCK)
        .execute(pool)
        .await
        .expect("unlock");
}

async fn cart_with_hoodie(repo: &SqlxCatalogRepository) -> rustashop_domain::Cart {
    let currency = Currency::new("EUR").expect("EUR");
    let cart = repo.create_cart(&currency).await.expect("create");
    let (variant, product_name) = repo
        .find_variant_for_cart(HOODIE_VARIANT)
        .await
        .expect("find variant")
        .expect("hoodie");
    let mut loaded = repo
        .find_cart_by_id(&cart.id)
        .await
        .expect("load")
        .expect("cart");
    loaded.lines.push(CartLine {
        id: String::new(),
        cart_id: loaded.id.clone(),
        variant_id: variant.id,
        quantity: 1,
        unit_price: variant.price,
        product_name,
        variant_sku: variant.sku,
    });
    repo.save_cart(&loaded).await.expect("save");
    repo.find_cart_by_id(&cart.id)
        .await
        .expect("reload")
        .expect("cart")
}

#[tokio::test]
async fn cart_token_save_delete_and_missing_paths() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    let currency = Currency::new("EUR").expect("EUR");
    let cart = repo.create_cart(&currency).await.expect("create");
    assert!(CartRepository::find_by_token(&repo, &cart.token)
        .await
        .expect("token")
        .is_some());
    assert!(CartRepository::find_by_token(&repo, "missing-token")
        .await
        .expect("missing")
        .is_none());
    assert!(repo
        .find_cart_by_id("00000000-0000-0000-0000-000000000000")
        .await
        .expect("id")
        .is_none());
    assert!(repo
        .find_variant_for_cart("00000000-0000-0000-0000-000000000000")
        .await
        .expect("variant")
        .is_none());

    let with_line = cart_with_hoodie(&repo).await;
    assert_eq!(with_line.lines.len(), 1);
    assert_ne!(with_line.lines[0].id, "");

    let empty = repo.create_cart(&currency).await.expect("empty cart");
    CartRepository::delete(&repo, &empty.id)
        .await
        .expect("delete");
    assert!(repo
        .find_cart_by_id(&empty.id)
        .await
        .expect("after delete")
        .is_none());

    let mut ghost = with_line;
    ghost.id = "00000000-0000-0000-0000-000000000099".to_owned();
    ghost.status = CartStatus::Open;
    ghost.lines.clear();
    let save_missing = repo.save_cart(&ghost).await.expect_err("missing save");
    assert!(matches!(
        save_missing,
        PersistenceError::NotFound { entity: "cart", .. }
    ));
    let _ = Money::new(0, currency);
    unlock(&pool).await;
}

#[tokio::test]
async fn checkout_and_order_edge_paths() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    let currency = Currency::new("EUR").expect("EUR");
    let cart = cart_with_hoodie(&repo).await;

    let missing = repo
        .checkout_cart("00000000-0000-0000-0000-000000000000", None)
        .await
        .expect_err("missing cart");
    assert!(matches!(
        missing,
        PersistenceError::NotFound { entity: "cart", .. }
    ));

    let empty = repo.create_cart(&currency).await.expect("empty cart");
    let empty_err = repo
        .checkout_cart(&empty.id, None)
        .await
        .expect_err("empty");
    assert!(matches!(empty_err, PersistenceError::InvalidInput { .. }));

    let order = repo
        .checkout_cart(&cart.id, Some("sqlx-cart-key-1"))
        .await
        .expect("checkout");
    assert_eq!(order.lines.len(), 1);
    let replay = repo
        .checkout_cart(&cart.id, Some("sqlx-cart-key-1"))
        .await
        .expect("replay");
    assert_eq!(replay.id, order.id);

    let conflict = repo
        .checkout_cart(&cart.id, None)
        .await
        .expect_err("checked out");
    assert!(matches!(
        conflict,
        PersistenceError::Conflict {
            constraint: "cart_status"
        }
    ));

    let get = repo.get_order(&order.id).await.expect("get order");
    assert_eq!(get.id, order.id);
    let missing_order = repo
        .get_order("00000000-0000-0000-0000-000000000000")
        .await
        .expect_err("missing order");
    assert!(matches!(
        missing_order,
        PersistenceError::NotFound {
            entity: "order",
            ..
        }
    ));

    let listed = repo
        .list_orders(serenade_contracts::PageRequest::first(10))
        .await
        .expect("list");
    assert_ne!(listed.len(), 0);
    unlock(&pool).await;
}

#[tokio::test]
async fn order_state_update_and_invalid_ids() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    let cart = cart_with_hoodie(&repo).await;
    let order = repo
        .checkout_cart(&cart.id, Some("sqlx-state-key"))
        .await
        .expect("checkout");

    let paid = repo
        .update_order_state(&order.id, rustashop_domain::OrderState::Paid)
        .await
        .expect("paid");
    assert_eq!(paid.state, "paid");
    assert!(repo
        .update_order_state("not-a-uuid", rustashop_domain::OrderState::Paid)
        .await
        .is_err());
    assert!(matches!(
        repo.update_order_state(
            "00000000-0000-0000-0000-000000000000",
            rustashop_domain::OrderState::Paid
        )
        .await,
        Err(PersistenceError::NotFound {
            entity: "order",
            ..
        })
    ));
    assert!(repo.get_order("not-a-uuid").await.is_err());
    assert!(repo.checkout_cart("not-a-uuid", None).await.is_err());

    let unknown_key = repo
        .checkout_cart(&cart.id, Some("sqlx-unknown-after-checkout"))
        .await
        .expect_err("unknown key after checkout");
    assert!(matches!(
        unknown_key,
        PersistenceError::Conflict {
            constraint: "cart_status"
        }
    ));
    unlock(&pool).await;
}

#[tokio::test]
async fn checkout_multi_line_and_list_offset() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    let currency = Currency::new("EUR").expect("EUR");
    let _first = {
        let cart = cart_with_hoodie(&repo).await;
        repo.checkout_cart(&cart.id, Some("sqlx-offset-1"))
            .await
            .expect("first order")
    };

    let multi = repo.create_cart(&currency).await.expect("multi cart");
    let (hoodie, hoodie_name) = repo
        .find_variant_for_cart(HOODIE_VARIANT)
        .await
        .expect("hoodie")
        .expect("hoodie variant");
    let (mug, mug_name) = repo
        .find_variant_for_cart(MUG_VARIANT)
        .await
        .expect("mug")
        .expect("mug variant");
    let mut multi_loaded = repo
        .find_cart_by_id(&multi.id)
        .await
        .expect("load multi")
        .expect("cart");
    multi_loaded.lines.push(CartLine {
        id: String::new(),
        cart_id: multi.id.clone(),
        variant_id: hoodie.id,
        quantity: 1,
        unit_price: hoodie.price,
        product_name: hoodie_name,
        variant_sku: hoodie.sku,
    });
    multi_loaded.lines.push(CartLine {
        id: String::new(),
        cart_id: multi.id.clone(),
        variant_id: mug.id,
        quantity: 2,
        unit_price: mug.price,
        product_name: mug_name,
        variant_sku: mug.sku,
    });
    repo.save_cart(&multi_loaded).await.expect("save multi");
    let multi_order = repo
        .checkout_cart(&multi.id, None)
        .await
        .expect("checkout multi");
    assert_eq!(multi_order.lines.len(), 2);

    let offset = repo
        .list_orders(serenade_contracts::PageRequest {
            limit: 1,
            offset: 1,
        })
        .await
        .expect("offset");
    assert_eq!(offset.len(), 1);
    unlock(&pool).await;
}

#[tokio::test]
async fn cart_save_preserves_line_id_and_rejects_bad_ids() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    let with_line = cart_with_hoodie(&repo).await;
    let line_id = with_line.lines[0].id.clone();
    assert_ne!(line_id, "");
    CartRepository::save(&repo, &with_line)
        .await
        .expect("re-save");
    let again = repo
        .find_cart_by_id(&with_line.id)
        .await
        .expect("reload 2")
        .expect("cart");
    assert_eq!(again.lines[0].id, line_id);

    let bad = "not-a-uuid".to_owned();
    assert!(CartRepository::delete(&repo, &bad).await.is_err());
    assert!(repo.find_variant_for_cart("not-a-uuid").await.is_err());
    unlock(&pool).await;
}

#[tokio::test]
async fn checkout_copies_and_clears_customer_id() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    insert_customer(&pool).await;
    let mut cart = cart_with_hoodie(&repo).await;
    cart.customer_id = Some(CUSTOMER_ID.to_owned());
    repo.save_cart(&cart).await.expect("save customer");
    let loaded = repo
        .find_cart_by_id(&cart.id)
        .await
        .expect("reload")
        .expect("cart");
    assert_eq!(loaded.customer_id.as_deref(), Some(CUSTOMER_ID));

    let order = repo
        .checkout_cart(&cart.id, Some("sqlx-customer-key"))
        .await
        .expect("checkout");
    assert_eq!(order.customer_id.as_deref(), Some(CUSTOMER_ID));
    assert_eq!(
        repo.get_order(&order.id)
            .await
            .expect("get")
            .customer_id
            .as_deref(),
        Some(CUSTOMER_ID)
    );

    let mut open = cart_with_hoodie(&repo).await;
    open.customer_id = Some(CUSTOMER_ID.to_owned());
    repo.save_cart(&open).await.expect("set");
    open.customer_id = None;
    repo.save_cart(&open).await.expect("clear");
    let after = repo
        .find_cart_by_id(&open.id)
        .await
        .expect("reload clear")
        .expect("cart");
    assert!(after.customer_id.is_none());
    unlock(&pool).await;
}

#[tokio::test]
async fn checkout_concurrent_same_cart_same_key() {
    let Some((pool, repo)) = race_repo().await else {
        return;
    };
    let cart = cart_with_hoodie(&repo).await;
    let key = "sqlx-race-same-cart";
    let (a, b) = tokio::join!(
        repo.checkout_cart(&cart.id, Some(key)),
        repo.checkout_cart(&cart.id, Some(key)),
    );
    let a = a.expect("checkout a");
    let b = b.expect("checkout b");
    assert_eq!(a.id, b.id);
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*)::bigint FROM "order" WHERE idempotency_key = $1"#)
            .bind(key)
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count, 1);
    unlock(&pool).await;
}

#[tokio::test]
async fn checkout_concurrent_two_carts_same_key() {
    let Some((pool, repo)) = race_repo().await else {
        return;
    };
    let cart_a = cart_with_hoodie(&repo).await;
    let cart_b = cart_with_hoodie(&repo).await;
    let key = "sqlx-race-two-carts";
    let (a, b) = tokio::join!(
        repo.checkout_cart(&cart_a.id, Some(key)),
        repo.checkout_cart(&cart_b.id, Some(key)),
    );
    let a = a.expect("checkout a");
    let b = b.expect("checkout b");
    assert_eq!(a.id, b.id);
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*)::bigint FROM "order" WHERE idempotency_key = $1"#)
            .bind(key)
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count, 1);
    unlock(&pool).await;
}

#[tokio::test]
async fn find_cart_and_order_reject_corrupt_rows() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    let currency = Currency::new("EUR").expect("EUR");
    let cart = repo.create_cart(&currency).await.expect("create");
    sqlx::query("UPDATE cart SET currency = '12A' WHERE id = $1::uuid")
        .bind(&cart.id)
        .execute(&pool)
        .await
        .expect("corrupt currency");
    assert!(matches!(
        repo.find_cart_by_id(&cart.id).await,
        Err(PersistenceError::InvalidInput { .. })
    ));

    let cart2 = repo.create_cart(&currency).await.expect("create2");
    sqlx::query("ALTER TABLE cart DROP CONSTRAINT IF EXISTS cart_status_check")
        .execute(&pool)
        .await
        .expect("drop status check");
    sqlx::query("UPDATE cart SET status = 'nope' WHERE id = $1::uuid")
        .bind(&cart2.id)
        .execute(&pool)
        .await
        .expect("corrupt status");
    assert!(matches!(
        repo.find_cart_by_id(&cart2.id).await,
        Err(PersistenceError::InvalidInput { .. })
    ));

    let with_line = cart_with_hoodie(&repo).await;
    let order = repo
        .checkout_cart(&with_line.id, Some("sqlx-corrupt-order"))
        .await
        .expect("checkout");
    sqlx::query(r#"UPDATE "order" SET currency = 'ZZ' WHERE id = $1::uuid"#)
        .bind(&order.id)
        .execute(&pool)
        .await
        .expect("corrupt order");
    assert!(matches!(
        repo.get_order(&order.id).await,
        Err(PersistenceError::InvalidInput { .. })
    ));
    unlock(&pool).await;
}

#[tokio::test]
async fn checkout_rejects_overflow_line_total() {
    let Some((pool, repo)) = exclusive_repo().await else {
        return;
    };
    let cart = cart_with_hoodie(&repo).await;
    sqlx::query(
        "UPDATE cart_line SET unit_price_minor = $1, quantity = 2 WHERE cart_id = $2::uuid",
    )
    .bind(i64::MAX)
    .bind(&cart.id)
    .execute(&pool)
    .await
    .expect("overflow price");
    let err = repo
        .checkout_cart(&cart.id, None)
        .await
        .expect_err("overflow");
    assert!(matches!(err, PersistenceError::InvalidInput { .. }));
    unlock(&pool).await;
}

#[tokio::test]
async fn checkout_checked_out_cart_returns_existing_order_in_txn() {
    let Some((pool, repo)) = race_repo().await else {
        return;
    };
    let cart = cart_with_hoodie(&repo).await;
    let key = "sqlx-checked-out-in-txn";
    let mut hold = pool.begin().await.expect("begin hold");
    sqlx::query("SELECT id FROM cart WHERE id = $1::uuid FOR UPDATE")
        .bind(&cart.id)
        .fetch_one(&mut *hold)
        .await
        .expect("lock cart");

    let cart_id = cart.id.clone();
    let pool_clone = pool.clone();
    let join = tokio::spawn(async move {
        let repo = SqlxCatalogRepository::new(pool_clone);
        repo.checkout_cart(&cart_id, Some(key)).await
    });
    // Hold the row lock until checkout is blocked on FOR UPDATE.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    sqlx::query(
        r#"INSERT INTO "order" (
            number, cart_id, state, currency, items_total_minor, total_minor,
            idempotency_key, placed_at
        ) VALUES (
            'RS-hold-checked', $1::uuid, 'placed', 'EUR', 100, 100, $2, NOW()
        )"#,
    )
    .bind(&cart.id)
    .bind(key)
    .execute(&mut *hold)
    .await
    .expect("insert order");
    sqlx::query("UPDATE cart SET status = 'checked_out' WHERE id = $1::uuid")
        .bind(&cart.id)
        .execute(&mut *hold)
        .await
        .expect("mark checked out");
    hold.commit().await.expect("commit hold");

    let order = join.await.expect("join").expect("checkout");
    assert_eq!(order.idempotency_key.as_deref(), Some(key));
    unlock(&pool).await;
}

#[tokio::test]
async fn checkout_recovers_unique_idempotency_conflict() {
    let Some((pool, repo)) = race_repo().await else {
        return;
    };
    let cart_a = cart_with_hoodie(&repo).await;
    let cart_b = cart_with_hoodie(&repo).await;
    let key = "sqlx-unique-recover";
    let mut hold = pool.begin().await.expect("begin hold");
    sqlx::query(
        r#"INSERT INTO "order" (
            number, cart_id, state, currency, items_total_minor, total_minor,
            idempotency_key, placed_at
        ) VALUES (
            'RS-hold-unique', $1::uuid, 'placed', 'EUR', 100, 100, $2, NOW()
        )"#,
    )
    .bind(&cart_a.id)
    .bind(key)
    .execute(&mut *hold)
    .await
    .expect("insert uncommitted order");

    let cart_b_id = cart_b.id.clone();
    let pool_clone = pool.clone();
    let join = tokio::spawn(async move {
        let repo = SqlxCatalogRepository::new(pool_clone);
        repo.checkout_cart(&cart_b_id, Some(key)).await
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    hold.commit().await.expect("commit hold");

    let order = join.await.expect("join").expect("recover");
    assert_eq!(order.idempotency_key.as_deref(), Some(key));
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*)::bigint FROM "order" WHERE idempotency_key = $1"#)
            .bind(key)
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count, 1);
    unlock(&pool).await;
}

async fn insert_customer(pool: &PgPool) {
    sqlx::query("INSERT INTO customer (id, email) VALUES ($1::uuid, $2)")
        .bind(CUSTOMER_ID)
        .bind("buyer@example.com")
        .execute(pool)
        .await
        .expect("insert customer");
}

async fn race_repo() -> Option<(PgPool, SqlxCatalogRepository)> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return None;
    };
    let pool = PgPoolOptions::new()
        .max_connections(4)
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
    Some((pool.clone(), SqlxCatalogRepository::new(pool)))
}
