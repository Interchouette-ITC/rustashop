//! Product URL slug helpers via `serenade-string`.

use crate::DomainError;

/// Normalizes a product name or raw slug into a URL-safe lowercase slug.
///
/// # Errors
///
/// Returns [`DomainError::InvalidProductSlug`] when normalization yields an empty string.
pub fn product_slug(input: &str) -> Result<String, DomainError> {
    let slug = serenade_string::slug(input);
    if slug.is_empty() {
        return Err(DomainError::InvalidProductSlug(input.to_owned()));
    }
    Ok(slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_from_display_names() {
        assert_eq!(product_slug("Classic Hoodie").unwrap(), "classic-hoodie");
        assert_eq!(product_slug("  Hello World! ").unwrap(), "hello-world");
        assert_eq!(product_slug("HOODIE").unwrap(), "hoodie");
    }

    #[test]
    fn rejects_empty_after_normalize() {
        assert!(matches!(
            product_slug(""),
            Err(DomainError::InvalidProductSlug(_))
        ));
        assert!(matches!(
            product_slug("!!!"),
            Err(DomainError::InvalidProductSlug(_))
        ));
        assert!(matches!(
            product_slug("   "),
            Err(DomainError::InvalidProductSlug(_))
        ));
    }
}
