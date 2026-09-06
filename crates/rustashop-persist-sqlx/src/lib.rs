//! `SQLx` persistence: versioned SQL migrations, queries, and catalog repositories.

pub mod cart;
pub mod catalog;
pub mod checkout;
pub mod orders;
pub mod param;
pub mod raw_sql;

use sqlx::postgres::PgPool;

pub use catalog::SqlxCatalogRepository;
pub use param::{ensure_param, ensure_param_opt};
pub use raw_sql::{ALLOW_RAW_SQL_ENV, assert_raw_sql_allowed, execute_fragment, raw_sql_allowed};

/// SQL used by [`seed_catalog`] and `make db-seed`.
pub const CATALOG_SEED_SQL: &str = include_str!("../../../db/seeds/catalog.sql");

/// Applies embedded `SQLx` migrations against the given pool.
///
/// # Errors
///
/// Returns [`sqlx::migrate::MigrateError`] when migration execution fails.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!().run(pool).await
}

/// Inserts the catalog seed rows (`ON CONFLICT DO NOTHING`).
///
/// # Errors
///
/// Returns [`sqlx::Error`] when a statement fails.
pub async fn seed_catalog(pool: &PgPool) -> Result<(), sqlx::Error> {
    for statement in CATALOG_SEED_SQL.split(';') {
        let statement = statement.trim();
        if statement.is_empty() {
            continue;
        }
        sqlx::query(sqlx::AssertSqlSafe(statement.to_owned()))
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Connects with `DATABASE_URL` and returns a catalog repository.
///
/// # Errors
///
/// Returns [`MigrateError`] when the URL is missing or the database is unreachable.
pub async fn catalog_from_env() -> Result<SqlxCatalogRepository, MigrateError> {
    let url = require_database_url(std::env::var("DATABASE_URL"))?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await?;
    Ok(SqlxCatalogRepository::new(pool))
}

/// Connects with `DATABASE_URL` and runs embedded migrations.
///
/// # Errors
///
/// Returns [`MigrateError`] when the database is unreachable or migrations fail.
pub async fn migrate_from_env() -> Result<(), MigrateError> {
    let url = require_database_url(std::env::var("DATABASE_URL"))?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    migrate(&pool).await?;
    Ok(())
}

fn require_database_url(
    result: Result<String, std::env::VarError>,
) -> Result<String, MigrateError> {
    result.map_err(|_| MigrateError::MissingDatabaseUrl)
}

/// Migration runner errors.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// `DATABASE_URL` is not set.
    #[error("DATABASE_URL must be set")]
    MissingDatabaseUrl,
    /// Underlying `SQLx` connection error.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// Underlying `SQLx` migration error.
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

#[cfg(test)]
mod env_tests {
    use super::*;

    #[test]
    fn require_database_url_maps_missing() {
        assert!(matches!(
            require_database_url(Err(std::env::VarError::NotPresent)),
            Err(MigrateError::MissingDatabaseUrl)
        ));
        assert_eq!(
            require_database_url(Ok("postgres://x".into())).unwrap(),
            "postgres://x"
        );
    }
}
