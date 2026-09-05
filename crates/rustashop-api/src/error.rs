//! HTTP error mapping for the Actix API.

use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};
use rustashop_domain::DomainError;
use serde::Serialize;
use serenade_contracts::PersistenceError;

/// JSON error body (message + stable machine code).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    /// Human-readable error message.
    pub error: String,
    /// Stable machine-readable code (`not_found`, `conflict`, …).
    pub code: &'static str,
}

/// Handler failure mapped to an HTTP status.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// Missing or invalid admin bearer token.
    #[error("unauthorized")]
    Unauthorized,
    /// Requested aggregate is missing.
    #[error("not found")]
    NotFound,
    /// Unique constraint or already-checked-out cart.
    #[error("conflict")]
    Conflict,
    /// Request body or domain rule rejected.
    #[error("{0}")]
    Unprocessable(String),
    /// Persistence or unexpected failure (no internal details in Display).
    #[error("internal error")]
    Internal,
}

impl ApiError {
    /// Stable code for clients and `OpenAPI` docs.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Unprocessable(_) => "unprocessable",
            Self::Internal => "internal",
        }
    }

    pub(crate) fn from_persist(error: &PersistenceError) -> Self {
        match error {
            PersistenceError::NotFound { .. } => Self::NotFound,
            PersistenceError::InvalidInput { message } => Self::Unprocessable(message.clone()),
            PersistenceError::Conflict { .. } => Self::Conflict,
            PersistenceError::Internal { .. } => Self::Internal,
        }
    }

    pub(crate) fn from_domain(error: &DomainError) -> Self {
        match error {
            DomainError::InvalidCurrency(_)
            | DomainError::CurrencyMismatch { .. }
            | DomainError::InvalidQuantity(_)
            | DomainError::Overflow
            | DomainError::EmptyCart
            | DomainError::InvalidCartStatus(_)
            | DomainError::InvalidOrderState(_) => Self::Unprocessable(error.to_string()),
            DomainError::CartAlreadyCheckedOut => Self::Conflict,
            DomainError::LineNotFound(_) => Self::NotFound,
        }
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Unprocessable(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: self.to_string(),
            code: self.code(),
        })
    }
}

/// Serenade JSON response for a successful payload.
#[must_use]
pub fn json_response(status: u16, body: &impl Serialize) -> serenade_http::Response {
    let bytes = serde_json::to_vec(body).unwrap_or_else(|_| b"{\"error\":\"encode\"}".to_vec());
    serenade_http::Response::new(status)
        .with_header("content-type", "application/json")
        .with_body(bytes)
}

/// Serenade JSON response for an [`ApiError`].
#[must_use]
pub fn api_error_json_response(error: &ApiError) -> serenade_http::Response {
    json_response(
        error.status_code().as_u16(),
        &ErrorBody {
            error: error.to_string(),
            code: error.code(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_persist_and_domain_variants() {
        assert!(matches!(
            ApiError::from_persist(&PersistenceError::NotFound {
                entity: "cart",
                id: "1".into(),
            }),
            ApiError::NotFound
        ));
        assert!(matches!(
            ApiError::from_persist(&PersistenceError::InvalidInput {
                message: "bad".into(),
            }),
            ApiError::Unprocessable(_)
        ));
        assert!(matches!(
            ApiError::from_persist(&PersistenceError::Conflict { constraint: "x" }),
            ApiError::Conflict
        ));
        assert!(matches!(
            ApiError::from_persist(&PersistenceError::Internal {
                message: "db".into(),
            }),
            ApiError::Internal
        ));
        assert!(matches!(
            ApiError::from_domain(&DomainError::CartAlreadyCheckedOut),
            ApiError::Conflict
        ));
        assert!(matches!(
            ApiError::from_domain(&DomainError::LineNotFound("l".into())),
            ApiError::NotFound
        ));
        assert!(matches!(
            ApiError::from_domain(&DomainError::EmptyCart),
            ApiError::Unprocessable(_)
        ));
        assert_eq!(ApiError::Internal.code(), "internal");
        assert_eq!(
            ApiError::Internal.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
