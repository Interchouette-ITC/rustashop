//! Serenade listen entry point for the commerce HTTP API.

use rustashop_api::{
    bind_address, commerce_http_kernel, install_artefacts_present, shop_root, AdminApiPrefix,
    AdminAuthConfig, CommerceFrontConfig, ADMIN_API_PREFIX_ENV, ADMIN_TOKEN_ENV,
    ADMIN_TOKEN_ENV_ALT, BIND_ENV, DEFAULT_ADMIN_API_PREFIX, INSTALL_DIR_NAME,
    INSTALL_OFF_DIR_NAME,
};
use serenade_http_actix::{await_bound, bind_server};
use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

/// Compile-time persistence backend label for startup logs.
#[cfg(feature = "persist-sqlx")]
const PERSIST_BACKEND: &str = "sqlx";

/// Compile-time persistence backend label for startup logs.
#[cfg(feature = "persist-seaorm")]
const PERSIST_BACKEND: &str = "seaorm";

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx::query=warn,actix_server=warn"));
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(true)
        .with_writer(std::io::stdout)
        .init();
}

/// Redacts the password in a Postgres URL for safe logging.
fn redacted_database_url(raw: &str) -> String {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return "(unrecognized DATABASE_URL)".to_owned();
    };
    let Some((userinfo, host_and_path)) = rest.split_once('@') else {
        return format!("{scheme}://{rest}");
    };
    let user = userinfo.split(':').next().unwrap_or(userinfo);
    format!("{scheme}://{user}:***@{host_and_path}")
}

/// Maps bind failures to a clearer message (especially address-in-use).
fn bind_error(bind: &str, error: &std::io::Error) -> std::io::Error {
    if error.kind() == std::io::ErrorKind::AddrInUse {
        return std::io::Error::new(
            error.kind(),
            format!(
                "cannot bind {bind}: address already in use \
                 (another rustashop-api?). Free the port or set {BIND_ENV}"
            ),
        );
    }
    std::io::Error::new(error.kind(), format!("cannot bind {bind}: {error}"))
}

/// Starts commerce HTTP via Serenade [`bind_server`] / [`await_bound`] (listen helpers).
///
/// # Errors
///
/// Returns [`std::io::Error`] when the database is unreachable, bind fails, or
/// the accept loop fails.
#[allow(clippy::future_not_send)]
async fn run() -> std::io::Result<()> {
    let bind = bind_address();
    let version = env!("CARGO_PKG_VERSION");
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "(unset)".to_owned());

    info!("rustashop API {version}");
    info!("bind: http://{bind} (override with {BIND_ENV})");
    info!("persist: {PERSIST_BACKEND}");
    info!("database: {}", redacted_database_url(&database_url));

    let root = shop_root();
    let kernel = rustashop::boot_kernel(&root)
        .map_err(|error| std::io::Error::other(format!("serenade kernel boot failed: {error}")))?;
    info!(
        "serenade: env={} bundles={:?} status={}",
        kernel.environment(),
        kernel.bundle_names(),
        rustashop::kernel_status()
    );

    info!("health: http://{bind}/healthz");
    info!("openapi: http://{bind}/openapi.json");
    if install_artefacts_present(&root) {
        info!(
            "install API: /install/api/* (artefacts under {}/{INSTALL_DIR_NAME}/dist; rename to {INSTALL_OFF_DIR_NAME} after success)",
            root.display()
        );
    } else {
        info!(
            "install API: artefacts absent ({}/{INSTALL_DIR_NAME}/dist missing; expected if renamed to {INSTALL_OFF_DIR_NAME})",
            root.display()
        );
    }

    info!("connecting catalog repository...");
    let catalog = rustashop_persist::catalog_from_env()
        .await
        .map_err(std::io::Error::other)?;
    info!("catalog repository ready");
    let admin_auth = AdminAuthConfig::from_env();
    let admin_prefix = AdminApiPrefix::from_env();
    if admin_auth.is_configured() {
        info!(
            "admin: bearer configured; operator API under /v1/{{prefix}}/* (set {ADMIN_API_PREFIX_ENV})"
        );
    } else {
        info!(
            "admin: {ADMIN_TOKEN_ENV} (or {ADMIN_TOKEN_ENV_ALT}) unset - operator API returns 401"
        );
    }
    if admin_prefix.as_str() == DEFAULT_ADMIN_API_PREFIX {
        info!(
            "admin: using default API prefix `{DEFAULT_ADMIN_API_PREFIX}` - set {ADMIN_API_PREFIX_ENV} for installs"
        );
    } else {
        info!("admin: custom API prefix active ({ADMIN_API_PREFIX_ENV})");
    }

    let http_kernel = commerce_http_kernel(CommerceFrontConfig {
        catalog: Some(catalog),
        admin_auth,
        admin_prefix: admin_prefix.as_str().to_owned(),
        install_root: Some(root),
    });
    let server = bind_server(&bind, http_kernel).map_err(|error| bind_error(&bind, &error))?;
    info!("listening on http://{bind} (Serenade listen)");
    let result = await_bound(server).await;
    if let Err(error) = kernel.shutdown() {
        tracing::warn!("serenade kernel shutdown: {error}");
    }
    result
}

#[tokio::main]
async fn main() {
    init_tracing();
    if let Err(error) = run().await {
        error!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_database_password() {
        let raw = "postgres://rustashop:secret@127.0.0.1:5432/rustashop";
        assert_eq!(
            redacted_database_url(raw),
            "postgres://rustashop:***@127.0.0.1:5432/rustashop"
        );
    }

    #[test]
    fn bind_error_mentions_address_in_use() {
        let err = bind_error(
            "127.0.0.1:8080",
            &std::io::Error::new(std::io::ErrorKind::AddrInUse, "busy"),
        );
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
        assert!(err.to_string().contains(BIND_ENV));
    }

    #[test]
    fn bind_error_wraps_other_kinds() {
        let err = bind_error(
            "127.0.0.1:9",
            &std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope"),
        );
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(err.to_string().contains("127.0.0.1:9"));
    }
}
