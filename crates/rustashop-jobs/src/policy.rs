//! When the API process should spawn an in-process consumer.

use std::sync::Mutex;

use crate::messenger::MESSENGER_REDIS_URL_ENV;

/// Serializes tests that touch messenger Redis / inline-worker env vars.
pub static MESSENGER_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Env override: `1` force inline worker, `0` force external worker.
pub const INLINE_WORKER_ENV: &str = "RUSTASHOP_MESSENGER_INLINE_WORKER";

/// Default: inline when Redis URL unset (local/CI); external when Redis URL set.
#[must_use]
pub fn should_spawn_inline_worker() -> bool {
    match std::env::var(INLINE_WORKER_ENV) {
        Ok(value) if value == "1" || value.eq_ignore_ascii_case("true") => true,
        Ok(value) if value == "0" || value.eq_ignore_ascii_case("false") => false,
        _ => std::env::var(MESSENGER_REDIS_URL_ENV).is_err(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_inline_without_redis() {
        let _g = MESSENGER_ENV_LOCK.lock().expect("lock");
        // SAFETY: test-local env under [`MESSENGER_ENV_LOCK`].
        unsafe {
            std::env::remove_var(INLINE_WORKER_ENV);
            std::env::remove_var(MESSENGER_REDIS_URL_ENV);
        }
        assert!(should_spawn_inline_worker());
    }

    #[test]
    fn default_external_with_redis_url() {
        let _g = MESSENGER_ENV_LOCK.lock().expect("lock");
        unsafe {
            std::env::remove_var(INLINE_WORKER_ENV);
            std::env::set_var(MESSENGER_REDIS_URL_ENV, "redis://127.0.0.1:6379/0");
        }
        assert!(!should_spawn_inline_worker());
        unsafe {
            std::env::remove_var(MESSENGER_REDIS_URL_ENV);
        }
    }

    #[test]
    fn explicit_override() {
        let _g = MESSENGER_ENV_LOCK.lock().expect("lock");
        unsafe {
            std::env::set_var(MESSENGER_REDIS_URL_ENV, "redis://127.0.0.1:6379/0");
            std::env::set_var(INLINE_WORKER_ENV, "1");
        }
        assert!(should_spawn_inline_worker());
        unsafe {
            std::env::set_var(INLINE_WORKER_ENV, "0");
            std::env::remove_var(MESSENGER_REDIS_URL_ENV);
        }
        assert!(!should_spawn_inline_worker());
        unsafe {
            std::env::remove_var(INLINE_WORKER_ENV);
        }
    }
}
