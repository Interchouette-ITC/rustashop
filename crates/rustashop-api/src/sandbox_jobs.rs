//! Admin sandbox job HTTP create / status / audit.

use crate::admin_auth::AdminAuthConfig;
use crate::error::{ApiError, ErrorBody, api_error_json_response, json_response};
pub use rustashop_jobs::{
    CreateSandboxJobRequest, JOB_TYPE_CART_QUANTITY, JOB_TYPE_QUOTE, SandboxAdjustmentResponse,
    SandboxAuditRecord, SandboxJobLine, SandboxJobRegistry, SandboxJobResponse, SandboxJobStatus,
    SandboxProposalResponse, source_hash,
};
use rustashop_jobs::{
    SandboxJobEvent, SandboxJobHub, SandboxJobMessenger, SandboxJobWork, enqueue_sandbox_job,
};
use rustashop_sandbox::{CartLine, CartSnapshot, Money, quote_fixture_source};
use serenade_http::Response;

fn cart_from_request(body: &CreateSandboxJobRequest) -> Option<CartSnapshot> {
    let currency = body.currency.as_ref()?.clone();
    let lines = body.lines.as_ref()?;
    Some(CartSnapshot {
        currency: currency.clone(),
        lines: lines
            .iter()
            .map(|line| CartLine {
                sku: line.sku.clone(),
                quantity: line.quantity,
                unit_price: Money {
                    amount_minor: line.unit_price_minor,
                    currency: currency.clone(),
                },
            })
            .collect(),
    })
}

/// Creates a quote or autonomous cart-quantity job and returns the running job JSON.
pub async fn create_sandbox_job_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: &SandboxJobRegistry,
    hub: &SandboxJobHub,
    messenger: &SandboxJobMessenger,
    body: &[u8],
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    let request: CreateSandboxJobRequest = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => {
            return api_error_json_response(&ApiError::Unprocessable(
                "invalid create sandbox job body".into(),
            ));
        }
    };
    match request.job_type.as_str() {
        JOB_TYPE_QUOTE => create_quote_job(registry, hub, messenger, &request).await,
        JOB_TYPE_CART_QUANTITY => {
            crate::sandbox_autonomous::create_cart_quantity_job(registry, hub, messenger, &request)
                .await
        }
        _ => api_error_json_response(&ApiError::Unprocessable(
            "unsupported job_type (use quote or cart_quantity)".into(),
        )),
    }
}

async fn create_quote_job(
    registry: &SandboxJobRegistry,
    hub: &SandboxJobHub,
    messenger: &SandboxJobMessenger,
    request: &CreateSandboxJobRequest,
) -> Response {
    let Some(cart) = cart_from_request(request) else {
        return api_error_json_response(&ApiError::Unprocessable(
            "currency and at least one line are required".into(),
        ));
    };
    if cart.currency.trim().is_empty() || cart.lines.is_empty() {
        return api_error_json_response(&ApiError::Unprocessable(
            "currency and at least one line are required".into(),
        ));
    }

    let source = quote_fixture_source();
    let hash = source_hash(source);
    let job = registry.start_job(JOB_TYPE_QUOTE, &hash, "admin-bearer");
    let work = SandboxJobWork::Quote {
        job_id: job.id.clone(),
        cart,
        source: source.to_owned(),
    };
    if let Err(message) = enqueue_sandbox_job(messenger, work).await {
        let detail = format!("enqueue failed: {message}");
        hub.publish(&SandboxJobEvent::log(&job.id, detail.clone()));
        registry.finish_job(&job.id, SandboxJobStatus::Failed, None, Some(detail));
        return api_error_json_response(&ApiError::Internal);
    }

    json_response(202, &job)
}

/// Returns one job by id.
pub fn get_sandbox_job_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: &SandboxJobRegistry,
    job_id: &str,
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    registry.get(job_id).map_or_else(
        || api_error_json_response(&ApiError::NotFound),
        |job| json_response(200, &job),
    )
}

/// Lists recent audit rows.
pub fn list_sandbox_audit_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: &SandboxJobRegistry,
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    json_response(200, &registry.list_audit(50))
}

/// `POST /v1/{admin_api_prefix}/sandbox/jobs` `OpenAPI` stub.
#[utoipa::path(
    post,
    path = "/v1/{admin_api_prefix}/sandbox/jobs",
    tag = "sandbox",
    request_body = CreateSandboxJobRequest,
    security(("admin_bearer" = [])),
    responses(
        (status = 202, description = "Job accepted", body = SandboxJobResponse),
        (status = 422, description = "Invalid body", body = ErrorBody),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn create_sandbox_job() {}

/// `GET /v1/{admin_api_prefix}/sandbox/jobs/{id}` `OpenAPI` stub.
#[utoipa::path(
    get,
    path = "/v1/{admin_api_prefix}/sandbox/jobs/{id}",
    tag = "sandbox",
    params(("id" = String, Path, description = "Job id")),
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Job", body = SandboxJobResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody),
        (status = 404, description = "Unknown job", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn get_sandbox_job() {}

/// `GET /v1/{admin_api_prefix}/sandbox/audit` `OpenAPI` stub.
#[utoipa::path(
    get,
    path = "/v1/{admin_api_prefix}/sandbox/audit",
    tag = "sandbox",
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Audit rows", body = Vec<SandboxAuditRecord>),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn list_sandbox_audit() {}

#[cfg(test)]
mod tests {
    use super::*;
    use rustashop_jobs::{SandboxJobHub, run_quote_job};

    #[test]
    fn source_hash_is_stable() {
        assert_eq!(source_hash("abc"), source_hash("abc"));
        assert_ne!(source_hash("abc"), source_hash("abd"));
    }

    #[test]
    fn registry_start_and_finish() {
        let registry = SandboxJobRegistry::new();
        let job = registry.start_job(JOB_TYPE_QUOTE, "hash", "admin-bearer");
        assert_eq!(job.status, SandboxJobStatus::Running);
        assert!(format!("{registry:?}").contains("job_count"));
        registry.finish_job(&job.id, SandboxJobStatus::Succeeded, Some(vec![]), None);
        let done = registry.get(&job.id).expect("job");
        assert_eq!(done.status, SandboxJobStatus::Succeeded);
        let audit = registry.list_audit(10);
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].status, "succeeded");
    }

    #[test]
    fn registry_finish_failed_and_unknown_job() {
        let registry = SandboxJobRegistry::new();
        let job = registry.start_job(JOB_TYPE_QUOTE, "hash", "admin-bearer");
        registry.finish_job(&job.id, SandboxJobStatus::Failed, None, Some("boom".into()));
        let done = registry.get(&job.id).expect("job");
        assert_eq!(done.status, SandboxJobStatus::Failed);
        assert_eq!(done.error.as_deref(), Some("boom"));
        assert_eq!(registry.list_audit(1)[0].status, "failed");
        registry.finish_job("missing", SandboxJobStatus::Running, None, None);
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn registry_awaiting_commit_and_finalize() {
        let registry = SandboxJobRegistry::new();
        let job = registry.start_job(JOB_TYPE_CART_QUANTITY, "hash", "admin-bearer");
        registry.set_awaiting_commit(
            &job.id,
            SandboxProposalResponse {
                event_type: "cart.line_quantity_proposed".into(),
                cart_id: "c1".into(),
                product_id: "v1".into(),
                quantity: 2,
                operator: "set".into(),
            },
        );
        let waiting = registry.get(&job.id).expect("job");
        assert_eq!(waiting.status, SandboxJobStatus::AwaitingCommit);
        assert!(waiting.proposal.is_some());
        registry.finalize_proposal(&job.id, SandboxJobStatus::Committed);
        assert_eq!(
            registry.get(&job.id).expect("job").status,
            SandboxJobStatus::Committed
        );
        assert_eq!(registry.list_audit(1)[0].status, "committed");
        registry.finalize_proposal(&job.id, SandboxJobStatus::Discarded);
        assert_eq!(
            registry.get(&job.id).expect("job").status,
            SandboxJobStatus::Discarded
        );
        assert_eq!(registry.list_audit(1)[0].status, "discarded");
        // Cover status_label arms not reached by finish_job / set_awaiting_commit.
        registry.finalize_proposal(&job.id, SandboxJobStatus::Running);
        assert_eq!(registry.list_audit(1)[0].status, "running");
        registry.finalize_proposal(&job.id, SandboxJobStatus::AwaitingCommit);
        assert_eq!(registry.list_audit(1)[0].status, "awaiting_commit");
    }

    #[tokio::test]
    async fn create_get_list_response_auth_and_validation() {
        let auth = AdminAuthConfig::from_token("tok");
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let messenger = SandboxJobMessenger::new();

        assert_eq!(
            create_sandbox_job_response(&auth, None, &registry, &hub, &messenger, b"{}")
                .await
                .status(),
            401
        );
        assert_eq!(
            create_sandbox_job_response(
                &auth,
                Some("tok"),
                &registry,
                &hub,
                &messenger,
                b"not-json"
            )
            .await
            .status(),
            422
        );
        assert_eq!(
            create_sandbox_job_response(
                &auth,
                Some("tok"),
                &registry,
                &hub,
                &messenger,
                br#"{"job_type":"other","currency":"EUR","lines":[{"sku":"a","quantity":1,"unit_price_minor":1}]}"#
            )
            .await
            .status(),
            422
        );
        assert_eq!(
            create_sandbox_job_response(
                &auth,
                Some("tok"),
                &registry,
                &hub,
                &messenger,
                br#"{"job_type":"quote"}"#
            )
            .await
            .status(),
            422
        );
        assert_eq!(
            create_sandbox_job_response(
                &auth,
                Some("tok"),
                &registry,
                &hub,
                &messenger,
                br#"{"job_type":"quote","currency":"","lines":[]}"#
            )
            .await
            .status(),
            422
        );
        assert_eq!(
            create_sandbox_job_response(
                &auth,
                Some("tok"),
                &registry,
                &hub,
                &messenger,
                br#"{"job_type":"cart_quantity","cart_id":"c1","variant_id":"v1","quantity":2,"operator":"set"}"#
            )
            .await
            .status(),
            202
        );
        assert_eq!(messenger.transport().len(), 1);
        assert_eq!(
            get_sandbox_job_response(&auth, None, &registry, "x").status(),
            401
        );
        assert_eq!(
            get_sandbox_job_response(&auth, Some("tok"), &registry, "x").status(),
            404
        );
        assert_eq!(
            list_sandbox_audit_response(&auth, None, &registry).status(),
            401
        );
        assert_eq!(
            list_sandbox_audit_response(&auth, Some("tok"), &registry).status(),
            200
        );
    }

    #[tokio::test]
    async fn create_quote_job_enqueues_and_returns_202() {
        crate::sandbox_messenger::force_enqueue_failure(false);
        let auth = AdminAuthConfig::from_token("tok");
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let messenger = SandboxJobMessenger::new();
        let body = br#"{"job_type":"quote","currency":"EUR","lines":[{"sku":"HOODIE-M","quantity":1,"unit_price_minor":5000}]}"#;
        let response =
            create_sandbox_job_response(&auth, Some("tok"), &registry, &hub, &messenger, body)
                .await;
        assert_eq!(response.status(), 202);
        assert_eq!(messenger.transport().len(), 1);
    }

    #[tokio::test]
    async fn create_quote_job_reports_enqueue_failure() {
        crate::sandbox_messenger::force_enqueue_failure(true);
        let auth = AdminAuthConfig::from_token("tok");
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let messenger = SandboxJobMessenger::new();
        let body = br#"{"job_type":"quote","currency":"EUR","lines":[{"sku":"HOODIE-M","quantity":1,"unit_price_minor":5000}]}"#;
        let response =
            create_sandbox_job_response(&auth, Some("tok"), &registry, &hub, &messenger, body)
                .await;
        assert_eq!(response.status(), 500);
        crate::sandbox_messenger::force_enqueue_failure(false);
    }

    #[tokio::test]
    async fn run_quote_job_succeeds() {
        let _wasmer = rustashop_sandbox::WASMER_TEST_GATE.lock().await;
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let cart = CartSnapshot {
            currency: "EUR".into(),
            lines: vec![CartLine {
                sku: "HOODIE-M".into(),
                quantity: 2,
                unit_price: Money {
                    amount_minor: 5000,
                    currency: "EUR".into(),
                },
            }],
        };
        let job = registry.start_job(JOB_TYPE_QUOTE, "hash", "admin-bearer");
        run_quote_job(&registry, &hub, &job.id, &cart, quote_fixture_source()).await;
        let done = registry.get(&job.id).expect("job");
        assert_eq!(
            done.status,
            SandboxJobStatus::Succeeded,
            "error={:?}",
            done.error
        );
        assert_eq!(done.adjustments.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn openapi_stubs_are_callable() {
        create_sandbox_job();
        get_sandbox_job();
        list_sandbox_audit();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn run_quote_job_maps_guest_and_validation_failures() {
        let _wasmer = rustashop_sandbox::WASMER_TEST_GATE.lock().await;
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let cart = CartSnapshot {
            currency: "EUR".into(),
            lines: vec![CartLine {
                sku: "HOODIE-M".into(),
                quantity: 2,
                unit_price: Money {
                    amount_minor: 5000,
                    currency: "EUR".into(),
                },
            }],
        };

        let guest_fail = registry.start_job(JOB_TYPE_QUOTE, "bad", "admin");
        run_quote_job(
            &registry,
            &hub,
            &guest_fail.id,
            &cart,
            "raise SystemExit(1)",
        )
        .await;
        let failed = registry.get(&guest_fail.id).expect("job");
        assert_eq!(failed.status, SandboxJobStatus::Failed);
        assert!(
            failed
                .error
                .as_deref()
                .is_some_and(|message| message.contains("guest failed")),
            "error={:?}",
            failed.error
        );

        let validation_fail = registry.start_job(JOB_TYPE_QUOTE, "usd", "admin");
        let bad_currency = r#"import json,sys; json.dump([{"label":"x","amount_minor":-1,"currency":"USD"}], sys.stdout)"#;
        run_quote_job(&registry, &hub, &validation_fail.id, &cart, bad_currency).await;
        let failed = registry.get(&validation_fail.id).expect("job");
        assert_eq!(failed.status, SandboxJobStatus::Failed);
        assert!(
            failed
                .error
                .as_deref()
                .is_some_and(|message| message.contains("validation failed")),
            "error={:?}",
            failed.error
        );
    }
}
