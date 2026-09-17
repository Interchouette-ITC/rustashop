//! Map Serenade validator violations onto [`ApiError`].

use serenade_validator::{ConstraintViolationList, RecursiveValidator, Validatable};

use crate::error::ApiError;

/// Runs [`Validatable::validate`] and maps non-empty violations to 422.
///
/// # Errors
///
/// Returns [`ApiError::Unprocessable`] when any constraint fails.
pub fn validate_request(value: &impl Validatable) -> Result<(), ApiError> {
    let violations = value.validate(&RecursiveValidator);
    if violations.is_empty() {
        return Ok(());
    }
    Err(api_error_from_violations(&violations))
}

fn api_error_from_violations(violations: &ConstraintViolationList) -> ApiError {
    let message = violations
        .as_slice()
        .iter()
        .map(|violation| {
            if violation.property_path.is_empty() {
                violation.message.clone()
            } else {
                format!("{}: {}", violation.property_path, violation.message)
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    ApiError::Unprocessable(message)
}

#[cfg(test)]
mod tests {
    use serenade_validator::{Constraint, ConstraintViolationList, NotBlank, Validator, Violation};

    use super::*;

    struct BlankName {
        name: String,
    }

    impl Validatable for BlankName {
        fn validate(&self, validator: &dyn Validator) -> ConstraintViolationList {
            validator.validate_value(&self.name, "name", &[&NotBlank as &dyn Constraint])
        }
    }

    #[test]
    fn maps_violations_to_unprocessable() {
        let err = validate_request(&BlankName {
            name: String::new(),
        })
        .expect_err("blank");
        assert!(matches!(err, ApiError::Unprocessable(message) if message.contains("name")));
    }

    #[test]
    fn accepts_valid() {
        validate_request(&BlankName { name: "ok".into() }).expect("valid");
    }

    #[test]
    fn object_level_violation_omits_path_prefix() {
        let mut list = ConstraintViolationList::new();
        list.add(Violation::new("", "object invalid", "Custom"));
        assert!(matches!(
            api_error_from_violations(&list),
            ApiError::Unprocessable(ref message) if message == "object invalid"
        ));
    }
}
