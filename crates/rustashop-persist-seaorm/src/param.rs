//! Persist-param hygiene helper (NUL rejection).
//!
//! Available for opt-in use (raw SQL escape hatch, shared policy). Normal
//! repository queries rely on the query builder, not on calling this before
//! every filter.

use serenade_contracts::{PersistenceError, reject_unsafe_sql_param};

/// Rejects NUL in a string (interop hygiene, not SQL-injection protection).
///
/// # Errors
///
/// Returns [`PersistenceError::InvalidInput`] when checks are enabled and the
/// value contains NUL.
pub fn ensure_param(value: &str) -> Result<&str, PersistenceError> {
    reject_unsafe_sql_param(value)
}

/// Optional string parameter.
///
/// # Errors
///
/// Same as [`ensure_param`] when `Some`.
pub fn ensure_param_opt<S: AsRef<str> + ?Sized>(
    value: Option<&S>,
) -> Result<Option<&str>, PersistenceError> {
    match value {
        None => Ok(None),
        Some(value) => Ok(Some(ensure_param(value.as_ref())?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nul() {
        assert!(ensure_param("a\0b").is_err());
    }

    #[test]
    fn accepts_clean_opt() {
        assert_eq!(ensure_param("ok").unwrap(), "ok");
        assert_eq!(ensure_param_opt(None::<&str>).unwrap(), None);
        assert_eq!(ensure_param_opt(Some("ok")).unwrap(), Some("ok"));
        assert!(ensure_param_opt(Some("a\0b")).is_err());
    }
}
