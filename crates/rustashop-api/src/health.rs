//! Liveness probe.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// JSON body for `GET /healthz`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct HealthResponse {
    /// Liveness status.
    pub status: String,
    /// Serenade kernel integration marker from the `rustashop` crate (`serenade` after boot).
    pub kernel: String,
}

impl HealthResponse {
    /// Current liveness payload.
    #[must_use]
    pub fn ok() -> Self {
        Self {
            status: "ok".to_owned(),
            kernel: rustashop::kernel_status().to_owned(),
        }
    }
}

/// Serialized JSON body for Serenade / Actix health responses.
///
/// # Panics
///
/// Panics only if `HealthResponse` fails to serialize (it cannot for this type).
#[must_use]
pub fn health_json_body() -> Vec<u8> {
    serde_json::to_vec(&HealthResponse::ok()).expect("HealthResponse serializes")
}

/// `GET /healthz` `OpenAPI` path (served by the Serenade HTTP front controller).
#[utoipa::path(
    get,
    path = "/healthz",
    responses((status = 200, description = "Process is up", body = HealthResponse))
)]
#[allow(clippy::missing_const_for_fn)]
pub fn healthz() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_payload_and_stub() {
        let body = HealthResponse::ok();
        assert_eq!(body.status, "ok");
        let json = health_json_body();
        assert_ne!(json.as_slice(), &[] as &[u8]);
        healthz();
    }
}
