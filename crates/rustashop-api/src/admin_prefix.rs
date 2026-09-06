//! Configurable operator API URI segment (PrestaShop-style renameable admin path).

/// Env for the public API path segment under `/v1/{segment}/…`.
pub const ADMIN_API_PREFIX_ENV: &str = "RUSTASHOP_ADMIN_API_PREFIX";

/// Local-only default when [`ADMIN_API_PREFIX_ENV`] is unset.
///
/// Production installs must set a non-guessable value; do not leave this as the
/// only long-term public contract.
pub const DEFAULT_ADMIN_API_PREFIX: &str = "admin";

const RESERVED: &[&str] = &[
    "products",
    "carts",
    "checkout",
    "healthz",
    "openapi.json",
    "swagger-ui",
    "install",
];

/// Single URI segment for operator routes (`/v1/{this}/orders`, …).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminApiPrefix {
    segment: String,
}

impl AdminApiPrefix {
    /// Loads from [`ADMIN_API_PREFIX_ENV`], or [`DEFAULT_ADMIN_API_PREFIX`].
    ///
    /// # Panics
    ///
    /// Panics when the env value is invalid (empty, reserved, or bad charset).
    #[must_use]
    pub fn from_env() -> Self {
        let raw = std::env::var(ADMIN_API_PREFIX_ENV)
            .unwrap_or_else(|_| DEFAULT_ADMIN_API_PREFIX.to_owned());
        Self::parse(&raw).unwrap_or_else(|error| {
            panic!("{ADMIN_API_PREFIX_ENV} invalid: {error}");
        })
    }

    /// Builds from an explicit segment (tests).
    ///
    /// # Errors
    ///
    /// Returns a message when the segment is empty, reserved, or not
    /// `[A-Za-z0-9][A-Za-z0-9_-]{0,63}`.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let segment = raw.trim();
        if segment.len() > 64 {
            return Err("longer than 64 characters".to_owned());
        }
        let mut chars = segment.chars();
        let first = chars.next().ok_or_else(|| "empty".to_owned())?;
        if !first.is_ascii_alphanumeric() {
            return Err("must start with ASCII alphanumeric".to_owned());
        }
        if !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err("only ASCII alphanumeric, hyphen, underscore".to_owned());
        }
        if RESERVED
            .iter()
            .any(|word| segment.eq_ignore_ascii_case(word))
        {
            return Err(format!("reserved segment `{segment}`"));
        }
        Ok(Self {
            segment: segment.to_owned(),
        })
    }

    /// Path segment only (no leading slash).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.segment
    }

    /// Actix scope path: `/v1/{segment}`.
    #[must_use]
    pub fn scope_path(&self) -> String {
        format!("/v1/{}", self.segment)
    }

    /// Absolute path for a resource under this prefix (`/v1/{segment}/orders`).
    #[must_use]
    pub fn resource_path(&self, resource: &str) -> String {
        let resource = resource.trim_start_matches('/');
        format!("{}/{}", self.scope_path(), resource)
    }
}

/// No-op: admin JSON is registered on the Serenade front ([`crate::configure_serenade_front`]).
#[allow(clippy::missing_const_for_fn)]
pub fn configure_admin_routes(_cfg: &mut actix_web::web::ServiceConfig, _prefix: &AdminApiPrefix) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ADMIN_PREFIX_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[rstest::rstest]
    #[case::default_admin("admin", "/v1/admin", "/v1/admin/products")]
    #[case::opaque("bk-x9K2m", "/v1/bk-x9K2m", "/v1/bk-x9K2m/products")]
    fn accepts_valid_segment(#[case] raw: &str, #[case] scope: &str, #[case] products: &str) {
        let prefix = AdminApiPrefix::parse(raw).expect("valid");
        assert_eq!(prefix.as_str(), raw);
        assert_eq!(prefix.scope_path(), scope);
        assert_eq!(prefix.resource_path("products"), products);
    }

    #[rstest::rstest]
    #[case::empty("")]
    #[case::reserved_products("products")]
    #[case::path_traversal("../x")]
    #[case::slash("a/b")]
    #[case::bad_start("-nope")]
    fn rejects_invalid_segment(#[case] raw: &str) {
        assert!(AdminApiPrefix::parse(raw).is_err());
    }

    #[test]
    fn rejects_overlong_segment() {
        assert!(AdminApiPrefix::parse(&"a".repeat(65)).is_err());
    }

    #[test]
    fn from_env_defaults_to_admin() {
        let _guard = ADMIN_PREFIX_ENV_LOCK.lock().expect("lock");
        // SAFETY: test isolates admin prefix env.
        unsafe {
            std::env::remove_var(ADMIN_API_PREFIX_ENV);
        }
        assert_eq!(
            AdminApiPrefix::from_env().as_str(),
            DEFAULT_ADMIN_API_PREFIX
        );
        unsafe {
            std::env::set_var(ADMIN_API_PREFIX_ENV, "bk-fromenv1");
        }
        assert_eq!(AdminApiPrefix::from_env().as_str(), "bk-fromenv1");
        unsafe {
            std::env::remove_var(ADMIN_API_PREFIX_ENV);
        }
    }

    #[test]
    fn from_env_panics_on_invalid() {
        let _guard = ADMIN_PREFIX_ENV_LOCK.lock().expect("lock");
        unsafe {
            std::env::set_var(ADMIN_API_PREFIX_ENV, "products");
        }
        let panicked = std::panic::catch_unwind(AdminApiPrefix::from_env).is_err();
        unsafe {
            std::env::remove_var(ADMIN_API_PREFIX_ENV);
        }
        assert!(panicked, "invalid prefix must panic");
    }

    #[test]
    fn resource_path_strips_leading_slash() {
        let prefix = AdminApiPrefix::parse("admin").expect("admin");
        assert_eq!(prefix.resource_path("/orders"), "/v1/admin/orders");
    }

    #[test]
    fn rejects_reserved_openapi_and_install() {
        assert!(AdminApiPrefix::parse("openapi.json").is_err());
        assert!(AdminApiPrefix::parse("INSTALL").is_err());
    }

    #[actix_web::test]
    async fn configure_admin_routes_is_noop() {
        use actix_web::{App, test};
        let prefix = AdminApiPrefix::parse("opsfolder1").expect("prefix");
        let app =
            test::init_service(App::new().configure(|cfg| configure_admin_routes(cfg, &prefix)))
                .await;
        let req = test::TestRequest::get()
            .uri("/v1/opsfolder1/orders")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 404);
    }
}
