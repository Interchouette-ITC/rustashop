//! `SeaORM` migrations mirroring the `SQLx` `001_init.sql` schema.

mod m20250903_000001_init;
mod m20260903_000002_cart_status;

use sea_orm_migration::prelude::*;

/// Embedded SQL shared with the `SQLx` migration path.
pub const INIT_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rustashop-persist-sqlx/migrations/001_init.sql"
));

/// `SeaORM` migrator for the commerce schema.
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250903_000001_init::Migration),
            Box::new(m20260903_000002_cart_status::Migration),
        ]
    }
}
