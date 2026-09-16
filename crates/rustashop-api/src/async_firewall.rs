//! Async HTTP firewall using Serenade [`Authenticator`] (async twin of sync middleware).

use std::sync::Arc;

use serenade_http::{AsyncMiddleware, AsyncNext, BoxFuture, HttpError, Request, Response};
use serenade_security::{Authenticator, TOKEN_ATTRIBUTE, UsernamePasswordToken};

/// Async firewall: header → [`Authenticator`] → `_security_token` attribute.
///
/// Mirrors Serenade sync `FirewallMiddleware` for `AsyncHttpKernel` until an async
/// firewall lands in the framework.
pub struct AsyncFirewallMiddleware {
    header_name: String,
    authenticator: Arc<dyn Authenticator>,
    allow_anonymous: bool,
}

impl AsyncFirewallMiddleware {
    /// Creates a firewall reading `header_name` (commerce uses `Authorization`).
    #[must_use]
    pub fn new(header_name: impl Into<String>, authenticator: Arc<dyn Authenticator>) -> Self {
        Self {
            header_name: header_name.into(),
            authenticator,
            allow_anonymous: false,
        }
    }

    /// When `true`, missing credentials become an anonymous token instead of 401.
    #[must_use]
    pub const fn allow_anonymous(mut self, allow: bool) -> Self {
        self.allow_anonymous = allow;
        self
    }
}

impl AsyncMiddleware for AsyncFirewallMiddleware {
    fn process<'a>(
        &'a self,
        request: &'a mut Request,
        next: AsyncNext<'a>,
    ) -> BoxFuture<'a, Result<Response, HttpError>> {
        Box::pin(async move {
            let credentials = request.headers().get(&self.header_name);
            let token = if credentials.is_none() && self.allow_anonymous {
                UsernamePasswordToken::anonymous()
            } else {
                self.authenticator
                    .authenticate(credentials)
                    .map_err(|err| HttpError::status(401, err.to_string()))?
            };
            request.attributes_mut().insert(TOKEN_ATTRIBUTE, token);
            next.run(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenade_http::{AsyncHttpKernel, Method};
    use serenade_security::{InMemoryUser, SecurityError, TokenInterface};

    struct OkAuth;

    impl Authenticator for OkAuth {
        fn authenticate(
            &self,
            credentials: Option<&str>,
        ) -> Result<UsernamePasswordToken, SecurityError> {
            match credentials {
                Some("Bearer ok") => Ok(UsernamePasswordToken::authenticated(
                    InMemoryUser::new("admin", vec!["ROLE_ADMIN".to_owned()]),
                    "ok",
                )),
                _ => Err(SecurityError::Authentication {
                    message: "nope".to_owned(),
                }),
            }
        }
    }

    #[tokio::test]
    async fn missing_credentials_without_anonymous_returns_401() {
        let mut kernel = AsyncHttpKernel::from_async_fn(|_request: &mut Request| {
            Box::pin(async move { Ok(Response::text(200, "ok")) })
        });
        // Default allow_anonymous = false: no Authorization → authenticate(None) → 401.
        kernel.push_middleware(AsyncFirewallMiddleware::new(
            "Authorization",
            Arc::new(OkAuth),
        ));
        let response = kernel.handle(Request::new(Method::Get, "/")).await;
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn allow_anonymous_sets_anonymous_token() {
        let mut kernel = AsyncHttpKernel::from_async_fn(|request: &mut Request| {
            Box::pin(async move {
                let token = request
                    .attributes()
                    .get::<UsernamePasswordToken>(TOKEN_ATTRIBUTE)
                    .expect("token");
                assert!(!token.is_authenticated());
                Ok(Response::text(200, "ok"))
            })
        });
        kernel.push_middleware(
            AsyncFirewallMiddleware::new("Authorization", Arc::new(OkAuth)).allow_anonymous(true),
        );
        let response = kernel.handle(Request::new(Method::Get, "/")).await;
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn valid_bearer_stores_authenticated_token() {
        let mut kernel = AsyncHttpKernel::from_async_fn(|request: &mut Request| {
            Box::pin(async move {
                let token = request
                    .attributes()
                    .get::<UsernamePasswordToken>(TOKEN_ATTRIBUTE)
                    .expect("token");
                assert!(token.is_authenticated());
                assert_eq!(token.user().expect("user").user_identifier(), "admin");
                Ok(Response::text(200, "ok"))
            })
        });
        kernel.push_middleware(
            AsyncFirewallMiddleware::new("Authorization", Arc::new(OkAuth)).allow_anonymous(true),
        );
        let mut request = Request::new(Method::Get, "/");
        request.headers_mut().insert("Authorization", "Bearer ok");
        let response = kernel.handle(request).await;
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn invalid_bearer_returns_401() {
        let mut kernel = AsyncHttpKernel::from_async_fn(|_request: &mut Request| {
            Box::pin(async move { Ok(Response::text(200, "ok")) })
        });
        kernel.push_middleware(
            AsyncFirewallMiddleware::new("Authorization", Arc::new(OkAuth)).allow_anonymous(true),
        );
        let mut request = Request::new(Method::Get, "/");
        request.headers_mut().insert("Authorization", "Bearer bad");
        let response = kernel.handle(request).await;
        assert_eq!(response.status(), 401);
    }
}
