//! Admin bearer gate via Serenade security (`Authenticator` + voters).

use std::sync::Arc;

use serenade_http::Headers;
use serenade_security::{
    AccessDecisionManager, Authenticator, InMemoryUser, RoleVoter, SecurityError,
    UsernamePasswordToken,
};

use crate::error::ApiError;

/// Preferred env for the local admin bearer secret.
pub const ADMIN_TOKEN_ENV: &str = "RUSTASHOP_ADMIN_API_TOKEN";

/// Alternate env name from the admin API issue (`ADMIN_API_TOKEN`).
pub const ADMIN_TOKEN_ENV_ALT: &str = "ADMIN_API_TOKEN";

/// Access subject for operator routes (`RoleVoter` → `ROLE_ADMIN`).
pub const ADMIN_AREA_SUBJECT: &str = "admin.area";

/// Expected admin bearer token (empty rejects all admin calls).
#[derive(Clone)]
pub struct AdminAuthConfig {
    authenticator: Arc<dyn Authenticator>,
    expected_empty: bool,
    access: Arc<AccessDecisionManager>,
}

impl std::fmt::Debug for AdminAuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdminAuthConfig")
            .field("configured", &!self.expected_empty)
            .finish_non_exhaustive()
    }
}

impl Default for AdminAuthConfig {
    fn default() -> Self {
        Self::from_token("")
    }
}

impl AdminAuthConfig {
    /// Loads from `RUSTASHOP_ADMIN_API_TOKEN`, then `ADMIN_API_TOKEN`.
    #[must_use]
    pub fn from_env() -> Self {
        let token = std::env::var(ADMIN_TOKEN_ENV)
            .or_else(|_| std::env::var(ADMIN_TOKEN_ENV_ALT))
            .unwrap_or_default();
        Self::from_token(token)
    }

    /// Builds a config with an explicit token (tests).
    #[must_use]
    pub fn from_token(token: impl Into<String>) -> Self {
        let expected = token.into();
        let expected_empty = expected.is_empty();
        let authenticator: Arc<dyn Authenticator> = Arc::new(AdminBearerAuthenticator { expected });
        let mut access = AccessDecisionManager::new();
        access.add_voter(RoleVoter::new("ROLE_ADMIN", ADMIN_AREA_SUBJECT));
        Self {
            authenticator,
            expected_empty,
            access: Arc::new(access),
        }
    }

    /// Whether a non-empty token is configured.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        !self.expected_empty
    }

    /// Serenade authenticator for the async HTTP firewall.
    #[must_use]
    pub fn authenticator(&self) -> Arc<dyn Authenticator> {
        Arc::clone(&self.authenticator)
    }

    /// Requires a bearer secret matching the configured token and `admin.area` grant.
    ///
    /// # Errors
    ///
    /// Returns unauthorized when the token is unset, missing, wrong, or access is denied.
    pub fn authorize_bearer(&self, presented: Option<&str>) -> Result<(), ApiError> {
        let credentials = presented.map(|token| format!("Bearer {token}"));
        let token = self
            .authenticator
            .authenticate(credentials.as_deref())
            .map_err(|_| ApiError::Unauthorized)?;
        self.access
            .decide(&token, ADMIN_AREA_SUBJECT)
            .map_err(|_| ApiError::Unauthorized)?;
        Ok(())
    }
}

/// Serenade [`Authenticator`] for the shared admin API bearer secret.
#[derive(Clone, Debug)]
pub struct AdminBearerAuthenticator {
    expected: String,
}

impl Authenticator for AdminBearerAuthenticator {
    fn authenticate(
        &self,
        credentials: Option<&str>,
    ) -> Result<UsernamePasswordToken, SecurityError> {
        if self.expected.is_empty() {
            return Err(SecurityError::Authentication {
                message: "admin token unset".to_owned(),
            });
        }
        let Some(raw) = credentials else {
            return Err(SecurityError::Authentication {
                message: "missing credentials".to_owned(),
            });
        };
        let key = raw.strip_prefix("Bearer ").unwrap_or(raw).trim();
        if key.is_empty() || key != self.expected {
            return Err(SecurityError::Authentication {
                message: "invalid admin bearer".to_owned(),
            });
        }
        Ok(UsernamePasswordToken::authenticated(
            InMemoryUser::new("admin", vec!["ROLE_ADMIN".to_owned()]),
            key,
        ))
    }
}

/// Reads `Authorization: Bearer …` from Serenade request headers.
#[must_use]
pub fn bearer_from_headers(headers: &Headers) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenade_security::TokenInterface;

    #[test]
    fn debug_default_and_authenticator_surface() {
        let config = AdminAuthConfig::default();
        assert!(!config.is_configured());
        let debug = format!("{config:?}");
        assert!(debug.contains("AdminAuthConfig"));
        assert!(debug.contains("configured: false"));
        let _ = config.authenticator();
        let configured = AdminAuthConfig::from_token("secret");
        assert!(format!("{configured:?}").contains("configured: true"));
        assert!(
            configured
                .authenticator()
                .authenticate(Some("Bearer secret"))
                .is_ok()
        );
    }

    #[test]
    fn authorize_bearer_rejects_empty_presented_secret() {
        let config = AdminAuthConfig::from_token("secret");
        assert!(matches!(
            config.authorize_bearer(Some("")),
            Err(ApiError::Unauthorized)
        ));
        assert!(matches!(
            config.authorize_bearer(Some("   ")),
            Err(ApiError::Unauthorized)
        ));
    }

    #[test]
    fn authorize_bearer_denies_when_role_missing() {
        struct UserOnlyAuth;

        impl Authenticator for UserOnlyAuth {
            fn authenticate(
                &self,
                credentials: Option<&str>,
            ) -> Result<UsernamePasswordToken, SecurityError> {
                if credentials.is_none() {
                    return Err(SecurityError::Authentication {
                        message: "missing".to_owned(),
                    });
                }
                Ok(UsernamePasswordToken::authenticated(
                    InMemoryUser::new("user", vec!["ROLE_USER".to_owned()]),
                    "x",
                ))
            }
        }

        let mut access = AccessDecisionManager::new();
        access.add_voter(RoleVoter::new("ROLE_ADMIN", ADMIN_AREA_SUBJECT));
        let config = AdminAuthConfig {
            authenticator: Arc::new(UserOnlyAuth),
            expected_empty: false,
            access: Arc::new(access),
        };
        assert!(matches!(
            config.authorize_bearer(Some("anything")),
            Err(ApiError::Unauthorized)
        ));
    }

    #[test]
    fn authenticator_accepts_raw_secret_without_bearer_prefix() {
        let auth = AdminBearerAuthenticator {
            expected: "secret".to_owned(),
        };
        assert!(auth.authenticate(Some("secret")).is_ok());
    }

    #[test]
    fn from_token_is_configured() {
        let config = AdminAuthConfig::from_token("secret");
        assert!(config.is_configured());
        assert!(config.authorize_bearer(Some("secret")).is_ok());
    }

    #[test]
    fn from_env_reads_preferred_then_alt() {
        // SAFETY: test isolates admin token env keys.
        unsafe {
            std::env::remove_var(ADMIN_TOKEN_ENV);
            std::env::remove_var(ADMIN_TOKEN_ENV_ALT);
        }
        assert!(!AdminAuthConfig::from_env().is_configured());
        unsafe {
            std::env::set_var(ADMIN_TOKEN_ENV_ALT, "alt-secret");
        }
        assert!(
            AdminAuthConfig::from_env()
                .authorize_bearer(Some("alt-secret"))
                .is_ok()
        );
        unsafe {
            std::env::set_var(ADMIN_TOKEN_ENV, "preferred");
        }
        assert!(
            AdminAuthConfig::from_env()
                .authorize_bearer(Some("preferred"))
                .is_ok()
        );
        unsafe {
            std::env::remove_var(ADMIN_TOKEN_ENV);
            std::env::remove_var(ADMIN_TOKEN_ENV_ALT);
        }
    }

    #[rstest::rstest]
    #[case::unset(AdminAuthConfig::from_token(""), Some("x"))]
    #[case::missing(AdminAuthConfig::from_token("secret"), None)]
    #[case::wrong(AdminAuthConfig::from_token("secret"), Some("nope"))]
    fn authorize_bearer_rejects(#[case] config: AdminAuthConfig, #[case] presented: Option<&str>) {
        assert!(matches!(
            config.authorize_bearer(presented),
            Err(ApiError::Unauthorized)
        ));
    }

    #[test]
    fn authenticator_accepts_full_authorization_header() {
        let auth = AdminBearerAuthenticator {
            expected: "secret".to_owned(),
        };
        let token = auth.authenticate(Some("Bearer secret")).expect("ok");
        assert!(token.is_authenticated());
        assert_eq!(token.user().expect("user").user_identifier(), "admin");
    }

    #[test]
    fn bearer_from_headers_reads_authorization() {
        let mut headers = Headers::new();
        headers.insert("Authorization", "Bearer tok");
        assert_eq!(bearer_from_headers(&headers).as_deref(), Some("tok"));
        assert_eq!(bearer_from_headers(&Headers::new()), None);
        let mut blank = Headers::new();
        blank.insert("authorization", "Bearer   ");
        assert_eq!(bearer_from_headers(&blank), None);
    }
}
