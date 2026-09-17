//! Serenade listen entry point for the commerce HTTP API.

use rustashop_api::{
    ADMIN_API_PREFIX_ENV, ADMIN_TOKEN_ENV, ADMIN_TOKEN_ENV_ALT, AdminApiPrefix, AdminAuthConfig,
    BIND_ENV, CartHub, CommerceFrontConfig, DEFAULT_ADMIN_API_PREFIX, INSTALL_DIR_NAME,
    INSTALL_OFF_DIR_NAME, SandboxJobHub, SandboxJobRegistry, bind_address, bind_commerce_server,
    catalog_cache::CatalogCache,
    commerce_http_kernel, install_artefacts_present,
    order_mail::OrderMailer,
    public_rate_limit::PublicWriteRateLimiter,
    sandbox_messenger::{SandboxJobMessenger, spawn_configured_worker},
    shop_root,
};
use serenade_http::Readiness;
use serenade_http_actix::await_bound;
use serenade_kernel::Environment;
use serenade_observability::{LoggingConfig, LoggingGuard, init};
use tracing::{error, info};

/// Compile-time persistence backend label for startup logs.
#[cfg(feature = "persist-sqlx")]
const PERSIST_BACKEND: &str = "sqlx";

/// Compile-time persistence backend label for startup logs.
#[cfg(feature = "persist-seaorm")]
const PERSIST_BACKEND: &str = "seaorm";

/// Installs Serenade process logging (`var/log` + stderr). Keep the returned
/// value alive until process exit so file / `OTel` sinks flush.
fn init_logging() -> Result<LoggingHandles, serenade_observability::ObservabilityError> {
    let env_name = std::env::var("RUSTASHOP_ENV").unwrap_or_else(|_| "dev".to_owned());
    let environment = Environment::from_name(&env_name).unwrap_or(Environment::Dev);
    let mut config = LoggingConfig::for_environment(&environment, "var/log");
    if config.filter_directives.is_none()
        && std::env::var_os("SERENADE_LOG").is_none()
        && std::env::var_os("RUST_LOG").is_none()
    {
        config.filter_directives = Some("info,sqlx::query=warn,actix_server=warn".to_owned());
    }
    #[cfg(feature = "otel")]
    {
        use serenade_observability::{OtelConfig, init_with_otel};
        let mut otel = OtelConfig::new("rustashop-api");
        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            otel = otel.with_endpoint(endpoint);
        }
        let (logging, otel_guard) = init_with_otel(&config, &otel)?;
        return Ok(LoggingHandles {
            _logging: logging,
            _otel: Some(otel_guard),
        });
    }
    #[cfg(not(feature = "otel"))]
    {
        Ok(LoggingHandles {
            _logging: init(&config)?,
        })
    }
}

/// Holds logging (and optional `OTel`) guards for the process lifetime.
struct LoggingHandles {
    _logging: LoggingGuard,
    #[cfg(feature = "otel")]
    _otel: Option<serenade_observability::OtelGuard>,
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

    info!("health: http://{bind}/healthz (JSON liveness)");
    info!("ready: http://{bind}/readyz (LB readiness)");
    info!("openapi: http://{bind}/openapi.json");
    #[cfg(feature = "openapi-ui")]
    {
        info!("openapi UI: http://{bind}/swagger-ui/");
        info!("openapi UI: http://{bind}/redoc");
        info!("openapi UI: http://{bind}/rapidoc");
        info!("openapi UI: http://{bind}/scalar");
    }
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
    let hub = CartHub::new();
    let sandbox_hub = SandboxJobHub::new();
    let sandbox_registry = SandboxJobRegistry::new();
    let sandbox_messenger = SandboxJobMessenger::new();
    let _sandbox_worker = spawn_configured_worker(
        &sandbox_messenger,
        sandbox_registry.clone(),
        sandbox_hub.clone(),
    );
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

    let readiness = Readiness::new();
    let http_kernel = commerce_http_kernel(CommerceFrontConfig {
        catalog: Some(catalog.clone()),
        admin_auth: admin_auth.clone(),
        admin_prefix: admin_prefix.as_str().to_owned(),
        install_root: Some(root),
        cart_hub: Some(hub.clone()),
        sandbox_hub: Some(sandbox_hub.clone()),
        sandbox_registry: Some(sandbox_registry),
        sandbox_messenger: Some(sandbox_messenger),
        catalog_cache: Some(CatalogCache::from_env()),
        public_rate_limiter: Some(PublicWriteRateLimiter::from_env()),
        order_mailer: Some(OrderMailer::from_env()),
        readiness: readiness.clone(),
    });
    let bound = bind_commerce_server(
        &bind,
        http_kernel,
        hub,
        catalog,
        sandbox_hub,
        admin_auth,
        admin_prefix.as_str(),
    )
    .map_err(|error| bind_error(&bind, &error))?;
    info!("listening on http://{bind} (Serenade listen + cart/sandbox WS)");
    let result = await_bound(bound.server).await;
    readiness.mark_not_ready();
    if let Err(error) = kernel.shutdown() {
        tracing::warn!("serenade kernel shutdown: {error}");
    }
    result
}

#[tokio::main]
async fn main() {
    let _logging = match init_logging() {
        Ok(handles) => handles,
        Err(error) => {
            eprintln!("logging init failed: {error}");
            std::process::exit(1);
        }
    };
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
