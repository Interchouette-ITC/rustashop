//! Public mutating cart/checkout rate limits via `serenade-rate-limiter`.

use std::sync::Arc;
use std::time::Duration;

use serenade_http::{Headers, Response};
use serenade_rate_limiter::{
    ConsumeOrExceedError, InMemoryRateLimiterStorage, Policy, RateLimiterFactory,
    consume_or_exceed, too_many_requests,
};

/// Env: max tokens per window for public writes (default `120`). `0` disables limiting.
pub const PUBLIC_RATE_LIMIT_ENV: &str = "RUSTASHOP_PUBLIC_RATE_LIMIT";
/// Env: window length in seconds (default `60`).
pub const PUBLIC_RATE_WINDOW_SECS_ENV: &str = "RUSTASHOP_PUBLIC_RATE_WINDOW_SECS";

const DEFAULT_LIMIT: u32 = 120;
const DEFAULT_WINDOW_SECS: u64 = 60;
const FALLBACK_CLIENT_KEY: &str = "anon";

/// Shared factory for public write rate limits (in-memory storage by default).
///
/// Multi-node: swap storage for Serenade `RedisRateLimiterStorage` (feature `redis`).
#[derive(Clone)]
pub struct PublicWriteRateLimiter {
    factory: Option<RateLimiterFactory>,
}

impl std::fmt::Debug for PublicWriteRateLimiter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PublicWriteRateLimiter")
            .field("enabled", &self.factory.is_some())
            .finish()
    }
}

impl PublicWriteRateLimiter {
    /// Builds from env (high default so local/CI suites stay under the ceiling).
    #[must_use]
    pub fn from_env() -> Self {
        let limit = env_u32(PUBLIC_RATE_LIMIT_ENV, DEFAULT_LIMIT);
        let window_secs = env_u64(PUBLIC_RATE_WINDOW_SECS_ENV, DEFAULT_WINDOW_SECS).max(1);
        Self::with_policy(limit, Duration::from_secs(window_secs))
    }

    /// Explicit policy for tests (`limit == 0` disables).
    #[must_use]
    pub fn with_policy(limit: u32, window: Duration) -> Self {
        if limit == 0 {
            return Self { factory: None };
        }
        let Ok(policy) = Policy::fixed_window(limit, window) else {
            return Self { factory: None };
        };
        let storage = Arc::new(InMemoryRateLimiterStorage::new());
        let Ok(factory) = RateLimiterFactory::new("public_write", policy, storage) else {
            return Self { factory: None };
        };
        Self {
            factory: Some(factory),
        }
    }

    /// Consumes one token for `client_key`, or returns a `429` Serenade response.
    ///
    /// # Errors
    ///
    /// Returns a ready [`Response`] when the limiter rejects the client or storage fails.
    pub fn check(&self, client_key: &str) -> Result<(), Response> {
        let Some(factory) = &self.factory else {
            return Ok(());
        };
        let key = if client_key.is_empty() {
            FALLBACK_CLIENT_KEY
        } else {
            client_key
        };
        let limiter = factory
            .create(key)
            .map_err(|_| Response::text(500, "rate limiter key error"))?;
        match consume_or_exceed(&limiter, 1) {
            Ok(_) => Ok(()),
            Err(ConsumeOrExceedError::Exceeded(exceeded)) => {
                Err(too_many_requests(&exceeded.rate_limit))
            }
            Err(ConsumeOrExceedError::Limiter(_)) => {
                Err(Response::text(500, "rate limiter storage error"))
            }
        }
    }
}

/// Resolves a stable client key from proxy / test headers.
#[must_use]
pub fn client_key_from_headers(headers: &Headers) -> String {
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        let first = forwarded.split(',').next().unwrap_or("").trim();
        if !first.is_empty() {
            return first.to_owned();
        }
    }
    if let Some(real_ip) = headers.get("x-real-ip") {
        let trimmed = real_ip.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }
    if let Some(client_id) = headers.get("x-client-id") {
        let trimmed = client_id.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }
    FALLBACK_CLIENT_KEY.to_owned()
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_after_limit() {
        let limiter = PublicWriteRateLimiter::with_policy(2, Duration::from_secs(60));
        assert!(format!("{limiter:?}").contains("PublicWriteRateLimiter"));
        assert!(limiter.check("alice").is_ok());
        assert!(limiter.check("alice").is_ok());
        let err = limiter.check("alice").expect_err("third should 429");
        assert_eq!(err.status(), 429);
        assert!(limiter.check("bob").is_ok());
        assert!(limiter.check("").is_ok());
    }

    #[test]
    fn disabled_when_limit_zero() {
        let limiter = PublicWriteRateLimiter::with_policy(0, Duration::from_secs(60));
        for _ in 0..20 {
            assert!(limiter.check("flood").is_ok());
        }
    }

    #[test]
    fn zero_window_disables_factory() {
        let limiter = PublicWriteRateLimiter::with_policy(5, Duration::from_secs(0));
        assert!(limiter.check("anyone").is_ok());
    }

    #[test]
    fn from_env_builds_enabled_limiter() {
        let limiter = PublicWriteRateLimiter::from_env();
        assert!(limiter.check("from-env").is_ok());
    }

    #[test]
    fn client_key_prefers_forwarded_for() {
        let mut headers = Headers::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8");
        headers.insert("x-real-ip", "9.9.9.9");
        assert_eq!(client_key_from_headers(&headers), "1.2.3.4");
    }

    #[test]
    fn client_key_uses_real_ip_then_client_id() {
        let mut headers = Headers::new();
        headers.insert("x-forwarded-for", "  ,  ");
        headers.insert("x-real-ip", "10.0.0.1");
        assert_eq!(client_key_from_headers(&headers), "10.0.0.1");

        let mut headers = Headers::new();
        headers.insert("x-client-id", "suite-a");
        assert_eq!(client_key_from_headers(&headers), "suite-a");
    }

    #[test]
    fn client_key_falls_back_to_anon() {
        assert_eq!(
            client_key_from_headers(&Headers::new()),
            FALLBACK_CLIENT_KEY
        );
        let mut headers = Headers::new();
        headers.insert("x-real-ip", "   ");
        headers.insert("x-client-id", "");
        assert_eq!(client_key_from_headers(&headers), FALLBACK_CLIENT_KEY);
    }
}
