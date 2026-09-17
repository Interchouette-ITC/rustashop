//! Wasmer guest runners for enqueued sandbox jobs.

use rustashop_sandbox::{
    CartSnapshot, DomainEventDraft, LegacyHookInput, accept_validated_domain_event,
    apply_validated_adjustments, invoke_php_migration_hook, invoke_python_quote,
};

use crate::hub::{SandboxJobEvent, SandboxJobHub, SandboxProposalEventBody};
use crate::registry::{SandboxJobRegistry, SandboxJobStatus, SandboxProposalResponse};

/// Runs the Python quote guest and updates registry + hub.
pub async fn run_quote_job(
    registry: &SandboxJobRegistry,
    hub: &SandboxJobHub,
    job_id: &str,
    cart: &CartSnapshot,
    source: &str,
) {
    hub.publish(&SandboxJobEvent::log(
        job_id,
        "starting Wasmer Python quote",
    ));
    match invoke_python_quote(cart, source).await {
        Ok(raw) => match apply_validated_adjustments(cart, raw) {
            Ok(applied) => {
                hub.publish(&SandboxJobEvent::log(
                    job_id,
                    format!("validated {} adjustment(s)", applied.len()),
                ));
                registry.finish_job(job_id, SandboxJobStatus::Succeeded, Some(applied), None);
                hub.publish(&SandboxJobEvent::finished(job_id, "ok"));
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

/// Runs the PHP cart-quantity guest and stores an awaiting-commit proposal.
pub async fn run_cart_quantity_job(
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
