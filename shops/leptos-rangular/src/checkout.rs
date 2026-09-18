//! Checkout email field checks via rangular Host validators (#22 surface).

use rangular_host::{Regex, first_error, min_length, pattern, required};

/// Compiled once; reused for each email check.
fn email_pattern() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").expect("email pattern compiles")
    })
}

/// Validates an optional checkout email.
///
/// Empty / whitespace-only means “omit email” (matches Angular, which often sends none).
/// Non-empty values must pass `required` + `min_length(3)` + email `pattern`.
pub fn validate_checkout_email(raw: &str) -> Result<Option<String>, &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if let Some(msg) = first_error(&[
        required(trimmed),
        min_length(trimmed, 3),
        pattern(trimmed, email_pattern()),
    ]) {
        return Err(msg);
    }
    Ok(Some(trimmed.to_owned()))
}

/// Builds a browser-local idempotency key for `POST /v1/checkout`.
pub fn new_idempotency_key() -> String {
    format!(
        "rs-leptos-{}-{}",
        js_sys::Date::now(),
        js_sys::Math::random()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_email_omitted() {
        assert_eq!(validate_checkout_email(""), Ok(None));
        assert_eq!(validate_checkout_email("   "), Ok(None));
    }

    #[test]
    fn rejects_short_and_malformed() {
        assert!(validate_checkout_email("ab").is_err());
        assert!(validate_checkout_email("not-an-email").is_err());
        assert!(validate_checkout_email("@x.com").is_err());
    }

    #[test]
    fn accepts_simple_email() {
        assert_eq!(
            validate_checkout_email("buyer@example.test"),
            Ok(Some("buyer@example.test".into()))
        );
        assert_eq!(
            validate_checkout_email("  buyer@example.test  "),
            Ok(Some("buyer@example.test".into()))
        );
    }
}
