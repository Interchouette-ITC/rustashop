//! Cover `SeaORM` migration `down()` then restore with `up()`.

use rustashop_persist_seaorm::{Migrator, migrate};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;

const SCHEMA_LOCK: i64 = 874_523;

async fn exclusive_db() -> Option<DatabaseConnection> {
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
        .expect("reset");
    Some(db)
}

async fn unlock(db: &DatabaseConnection) {
    db.execute_unprepared(&format!("SELECT pg_advisory_unlock({SCHEMA_LOCK})"))
        .await
        .expect("unlock");
}

#[tokio::test]
async fn migrator_down_then_up_restores_schema() {
    let Some(db) = exclusive_db().await else {
        return;
    };
    migrate(&db).await.expect("up");
    Migrator::down(&db, None).await.expect("down");
    migrate(&db).await.expect("up again");
    db.execute_unprepared("SELECT 1 FROM cart LIMIT 1")
        .await
        .expect("cart exists after restore");
    unlock(&db).await;
}
