//! Admin bearer token validation via rangular Host validators.

use rangular_host::{first_error, min_length, required};

/// Validates a pasted admin API bearer token before save.
///
/// # Errors
///
/// Returns the first Host validator message when the value is empty or too short.
pub fn validate_admin_token(raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if let Some(msg) = first_error(&[required(trimmed), min_length(trimmed, 8)]) {
        return Err(msg);
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::validate_admin_token;

    #[test]
    fn rejects_empty_and_short() {
        assert!(validate_admin_token("").is_err());
        assert!(validate_admin_token("   ").is_err());
        assert!(validate_admin_token("short").is_err());
    }

    #[test]
    fn accepts_trimmed_token() {
        assert_eq!(
            validate_admin_token("  long-enough-token  ").unwrap(),
            "long-enough-token"
        );
    }
}
