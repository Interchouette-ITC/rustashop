//! Facade coverage for `migrate_from_env` and `catalog_from_env`.

use rustashop_persist::{PersistenceFactory, catalog_from_env, migrate_from_env};
use sqlx::postgres::PgPoolOptions;

const SCHEMA_LOCK: i64 = 874_520;

#[tokio::test]
async fn factory_and_free_fns_migrate_and_open_catalog() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };

    // Exclusive reset so migrate_from_env exercises a clean schema.
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
    drop(pool);

    PersistenceFactory
        .migrate_from_env()
        .await
        .expect("factory migrate");
    migrate_from_env().await.expect("migrate");

    let _ = PersistenceFactory
        .catalog_from_env()
        .await
        .expect("factory catalog");
    let _ = catalog_from_env().await.expect("catalog");
}
