//! Autonomous sandbox jobs: guest proposal + host-mediated commit.

use rustashop_persist::CatalogRepository;
use rustashop_sandbox::{
    CART_UPDATE_QUANTITY_HOOK, DomainEventDraft, LegacyHookInput, accept_validated_domain_event,
    invoke_php_migration_hook, php_migration_hook_source,
};
use serenade_http::Response;
use utoipa::ToSchema;

use crate::admin_auth::AdminAuthConfig;
use crate::carts::CartResponse;
use crate::error::{ApiError, ErrorBody, api_error_json_response, json_response};
use crate::realtime::{CartHub, CartRealtimeEvent};
use crate::sandbox_jobs::{
    CreateSandboxJobRequest, JOB_TYPE_CART_QUANTITY, SandboxJobRegistry, SandboxJobResponse,
    SandboxJobStatus, SandboxProposalResponse, source_hash,
};
use crate::sandbox_realtime::{SandboxJobEvent, SandboxJobHub, SandboxProposalEventBody};

/// Creates a `cart_quantity` job, spawns the PHP migration guest, returns 202.
pub fn create_cart_quantity_job(
    registry: &SandboxJobRegistry,
    hub: &SandboxJobHub,
    request: &CreateSandboxJobRequest,
) -> Response {
    let (cart_id, variant_id, quantity, operator) = match (
        request.cart_id.as_deref(),
        request.variant_id.as_deref(),
        request.quantity,
        request.operator.as_deref(),
    ) {
        (Some(cart_id), Some(variant_id), Some(quantity), Some(operator))
            if !cart_id.trim().is_empty()
                && !variant_id.trim().is_empty()
                && matches!(operator, "up" | "down" | "set") =>
        {
            (
                cart_id.to_owned(),
                variant_id.to_owned(),
                quantity,
                operator.to_owned(),
            )
        }
        _ => {
            return api_error_json_response(&ApiError::Unprocessable(
                "cart_quantity requires cart_id, variant_id, quantity, and operator (up|down|set)"
                    .into(),
            ));
        }
    };

    let source = php_migration_hook_source();
    let hash = source_hash(&source);
    let job = registry.start_job(JOB_TYPE_CART_QUANTITY, &hash, "admin-bearer");
    let job_id = job.id.clone();
    let input = LegacyHookInput {
        hook: CART_UPDATE_QUANTITY_HOOK.into(),
        cart_id,
        id_product: variant_id,
        quantity,
        operator,
    };
    let registry_runner = registry.clone();
    let hub_runner = hub.clone();

    tokio::spawn(async move {
        run_cart_quantity_job(&registry_runner, &hub_runner, &job_id, &input, &source).await;
    });

    json_response(202, &job)
}

async fn run_cart_quantity_job(
    registry: &SandboxJobRegistry,
    hub: &SandboxJobHub,
    job_id: &str,
    input: &LegacyHookInput,
    source: &str,
) {
    hub.publish(&SandboxJobEvent::log(
        job_id,
        "starting Wasmer PHP cart_quantity guest",
    ));
    match invoke_php_migration_hook(input, source).await {
        Ok(raw) => match accept_validated_domain_event(raw) {
            Ok(draft) => {
                let proposal = proposal_from_draft(&draft);
                hub.publish(&SandboxJobEvent::log(
                    job_id,
                    format!(
                        "validated proposal {} on cart {}",
                        proposal.event_type, proposal.cart_id
                    ),
                ));
                registry.set_awaiting_commit(job_id, proposal.clone());
                hub.publish(&SandboxJobEvent::proposal(
                    job_id,
                    SandboxProposalEventBody {
                        event_type: proposal.event_type,
                        cart_id: proposal.cart_id,
                        product_id: proposal.product_id,
                        quantity: proposal.quantity,
                        operator: proposal.operator,
                    },
                ));
            }
            Err(error) => {
                let message = format!("validation failed: {error:#}");
                hub.publish(&SandboxJobEvent::log(job_id, &message));
                registry.finish_job(job_id, SandboxJobStatus::Failed, None, Some(message));
                hub.publish(&SandboxJobEvent::finished(job_id, "error"));
            }
        },
        Err(error) => {
            let message = format!("guest failed: {error:#}");
            hub.publish(&SandboxJobEvent::log(job_id, &message));
            registry.finish_job(job_id, SandboxJobStatus::Failed, None, Some(message));
            hub.publish(&SandboxJobEvent::finished(job_id, "error"));
        }
    }
}

fn proposal_from_draft(draft: &DomainEventDraft) -> SandboxProposalResponse {
    SandboxProposalResponse {
        event_type: draft.event_type.clone(),
        cart_id: draft.cart_id.clone(),
        product_id: draft.product_id.clone(),
        quantity: draft.quantity,
        operator: draft.operator.clone(),
    }
}

/// Inputs for [`commit_sandbox_job_response`].
pub struct CommitSandboxJobContext<'a> {
    /// Admin bearer gate.
    pub auth: &'a AdminAuthConfig,
    /// Bearer token from the request.
    pub bearer: Option<&'a str>,
    /// Job registry.
    pub registry: &'a SandboxJobRegistry,
    /// Sandbox job push hub.
    pub hub: &'a SandboxJobHub,
    /// Optional cart push hub.
    pub cart_hub: Option<&'a CartHub>,
    /// Catalog for cart mutation.
    pub catalog: &'a CatalogRepository,
    /// Job id to commit.
    pub job_id: &'a str,
}

/// Host commit: apply a validated `cart_quantity` proposal to the live cart.
pub async fn commit_sandbox_job_response(ctx: CommitSandboxJobContext<'_>) -> Response {
    if let Err(error) = ctx.auth.authorize_bearer(ctx.bearer) {
        return api_error_json_response(&error);
    }
    let Some(job) = ctx.registry.get(ctx.job_id) else {
        return api_error_json_response(&ApiError::NotFound);
    };
    if job.status != SandboxJobStatus::AwaitingCommit {
        return api_error_json_response(&ApiError::Unprocessable(
            "job is not awaiting commit".into(),
        ));
    }
    let Some(proposal) = job.proposal.clone() else {
        return api_error_json_response(&ApiError::Unprocessable(
            "job has no proposal to commit".into(),
        ));
    };

    match apply_cart_quantity_proposal(ctx.catalog, ctx.cart_hub, &proposal).await {
        Ok(cart) => {
            ctx.registry
                .finalize_proposal(ctx.job_id, SandboxJobStatus::Committed);
            ctx.hub.publish(&SandboxJobEvent::log(
                ctx.job_id,
                format!("host committed quantity on cart {}", cart.id),
            ));
            ctx.hub
                .publish(&SandboxJobEvent::finished(ctx.job_id, "committed"));
            let updated = ctx.registry.get(ctx.job_id).unwrap_or(job);
            json_response(200, &CommitSandboxJobResponse { job: updated, cart })
        }
        Err(error) => api_error_json_response(&error),
    }
}

/// Discard a proposal without mutating commerce state.
pub fn discard_sandbox_job_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: &SandboxJobRegistry,
    hub: &SandboxJobHub,
    job_id: &str,
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    let Some(job) = registry.get(job_id) else {
        return api_error_json_response(&ApiError::NotFound);
    };
    if job.status != SandboxJobStatus::AwaitingCommit {
        return api_error_json_response(&ApiError::Unprocessable(
            "job is not awaiting commit".into(),
        ));
    }
    registry.finalize_proposal(job_id, SandboxJobStatus::Discarded);
    hub.publish(&SandboxJobEvent::log(job_id, "operator discarded proposal"));
    hub.publish(&SandboxJobEvent::finished(job_id, "discarded"));
    registry.get(job_id).map_or_else(
        || json_response(200, &job),
        |updated| json_response(200, &updated),
    )
}

async fn apply_cart_quantity_proposal(
    catalog: &CatalogRepository,
    cart_hub: Option<&CartHub>,
    proposal: &SandboxProposalResponse,
) -> Result<CartResponse, ApiError> {
    let mut cart = match catalog.find_cart_by_id(&proposal.cart_id).await {
        Ok(Some(cart)) => cart,
        Ok(None) => return Err(ApiError::NotFound),
        Err(error) => return Err(ApiError::from_persist(&error)),
    };
    let line = cart
        .lines
        .iter()
        .find(|line| line.variant_id == proposal.product_id)
        .ok_or(ApiError::NotFound)?;
    let line_id = line.id.clone();
    let current = line.quantity;
    let next = resolve_quantity(current, proposal.quantity, &proposal.operator)?;
    cart.update_line_quantity(&line_id, next)
        .map_err(|error| ApiError::from_domain(&error))?;
    let saved = match catalog.save_cart(&cart).await {
        Ok(()) => match catalog.find_cart_by_id(&proposal.cart_id).await {
            Ok(Some(cart)) => cart,
            Ok(None) => return Err(ApiError::NotFound),
            Err(error) => return Err(ApiError::from_persist(&error)),
        },
        Err(error) => return Err(ApiError::from_persist(&error)),
    };
    let response = CartResponse::try_from_cart(saved)?;
    if let Some(hub) = cart_hub {
        hub.publish(&CartRealtimeEvent::updated(response.clone()));
    }
    Ok(response)
}

fn resolve_quantity(current: i32, operand: u32, operator: &str) -> Result<i32, ApiError> {
    let operand = i32::try_from(operand).map_err(|_| {
        ApiError::Unprocessable("quantity operand does not fit signed 32-bit".into())
    })?;
    let next = match operator {
        "set" => operand,
        "up" => current.saturating_add(operand),
        "down" => current.saturating_sub(operand),
        other => {
            return Err(ApiError::Unprocessable(format!(
                "unsupported operator `{other}`"
            )));
        }
    };
    if next < 1 {
        return Err(ApiError::Unprocessable(
            "resolved quantity must be at least 1".into(),
        ));
    }
    Ok(next)
}

/// Commit response: updated job + cart snapshot after host apply.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct CommitSandboxJobResponse {
    /// Job after commit.
    pub job: SandboxJobResponse,
    /// Cart after host-mediated mutation.
    pub cart: CartResponse,
}

/// `POST /v1/{admin_api_prefix}/sandbox/jobs/{id}/commit` `OpenAPI` stub.
#[utoipa::path(
    post,
    path = "/v1/{admin_api_prefix}/sandbox/jobs/{id}/commit",
    params(("id" = String, Path, description = "Job id")),
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Committed", body = CommitSandboxJobResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody),
        (status = 404, description = "Unknown job or cart line", body = ErrorBody),
        (status = 422, description = "Not awaiting commit", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn commit_sandbox_job() {}

/// `POST /v1/{admin_api_prefix}/sandbox/jobs/{id}/discard` `OpenAPI` stub.
#[utoipa::path(
    post,
    path = "/v1/{admin_api_prefix}/sandbox/jobs/{id}/discard",
    params(("id" = String, Path, description = "Job id")),
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Discarded", body = SandboxJobResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody),
        (status = 404, description = "Unknown job", body = ErrorBody),
        (status = 422, description = "Not awaiting commit", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn discard_sandbox_job() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_quantity_operators() {
        assert_eq!(resolve_quantity(2, 3, "set").expect("set"), 3);
        assert_eq!(resolve_quantity(2, 3, "up").expect("up"), 5);
        assert_eq!(resolve_quantity(5, 2, "down").expect("down"), 3);
        assert!(resolve_quantity(1, 1, "down").is_err());
        assert!(resolve_quantity(1, 1, "nope").is_err());
    }

    #[test]
    fn openapi_stubs_are_callable() {
        commit_sandbox_job();
        discard_sandbox_job();
    }
}
