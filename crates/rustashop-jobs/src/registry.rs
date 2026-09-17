//! Sandbox job types and in-process registry (Redis status mirror when configured).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use rustashop_sandbox::Adjustment;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::status_store;

/// Fixed job type for the Python quote fixture.
pub const JOB_TYPE_QUOTE: &str = "quote";
/// Autonomous cart-quantity job (PHP migration guest → host commit).
pub const JOB_TYPE_CART_QUANTITY: &str = "cart_quantity";

/// Request body for creating a sandbox job.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateSandboxJobRequest {
    /// Job type (`quote` or `cart_quantity`).
    pub job_type: String,
    /// Cart currency for the quote fixture.
    #[serde(default)]
    pub currency: Option<String>,
    /// Cart lines for the quote guest snapshot.
    #[serde(default)]
    pub lines: Option<Vec<SandboxJobLine>>,
    /// Target cart id for `cart_quantity`.
    #[serde(default)]
    pub cart_id: Option<String>,
    /// Variant id passed to the migration guest as legacy `id_product`.
    #[serde(default)]
    pub variant_id: Option<String>,
    /// Quantity operand for `cart_quantity`.
    #[serde(default)]
    pub quantity: Option<u32>,
    /// Operator for `cart_quantity` (`up` / `down` / `set`).
    #[serde(default)]
    pub operator: Option<String>,
}

/// One cart line in the create-job body.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SandboxJobLine {
    /// SKU.
    pub sku: String,
    /// Quantity.
    pub quantity: u32,
    /// Unit price in minor units.
    pub unit_price_minor: i64,
}

/// Job status returned to the admin UI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxJobStatus {
    /// Runner still working.
    Running,
    /// Guest finished; host validated adjustments (quote path).
    Succeeded,
    /// Guest proposal validated; waiting for host commit.
    AwaitingCommit,
    /// Host applied the proposal to commerce state.
    Committed,
    /// Operator discarded the proposal without mutating commerce.
    Discarded,
    /// Guest or validation failed.
    Failed,
}

/// Validated domain-event proposal awaiting host commit.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct SandboxProposalResponse {
    /// Domain event type.
    pub event_type: String,
    /// Target cart id.
    pub cart_id: String,
    /// Variant id (legacy `id_product` in the guest ABI).
    pub product_id: String,
    /// Quantity operand.
    pub quantity: u32,
    /// Operator (`up` / `down` / `set`).
    pub operator: String,
}

/// Public job view.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SandboxJobResponse {
    /// Job id.
    pub id: String,
    /// Job type.
    pub job_type: String,
    /// Lifecycle status.
    pub status: SandboxJobStatus,
    /// Fingerprint of the fixture source used.
    pub source_hash: String,
    /// Host-validated adjustments when quote succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adjustments: Option<Vec<SandboxAdjustmentResponse>>,
    /// Validated proposal when awaiting commit / after commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal: Option<SandboxProposalResponse>,
    /// Error message when failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Adjustment JSON for admin.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SandboxAdjustmentResponse {
    /// Label.
    pub label: String,
    /// Signed minor units.
    pub amount_minor: i64,
    /// Currency.
    pub currency: String,
}

impl From<Adjustment> for SandboxAdjustmentResponse {
    fn from(value: Adjustment) -> Self {
        Self {
            label: value.label,
            amount_minor: value.amount_minor,
            currency: value.currency,
        }
    }
}

/// One audit row (process-local until a persist table lands).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SandboxAuditRecord {
    /// Job id.
    pub job_id: String,
    /// Actor label (`admin-bearer` when authorized).
    pub actor: String,
    /// Job type.
    pub job_type: String,
    /// Source fingerprint.
    pub source_hash: String,
    /// Final status string.
    pub status: String,
    /// Unix seconds when the job was created.
    pub created_at_unix: u64,
}

#[derive(Clone)]
struct JobRecord {
    response: SandboxJobResponse,
}

/// In-process job + audit store shared by HTTP and the runner.
#[derive(Clone, Default)]
pub struct SandboxJobRegistry {
    inner: Arc<Mutex<RegistryInner>>,
}

#[derive(Default)]
struct RegistryInner {
    jobs: HashMap<String, JobRecord>,
    audit: Vec<SandboxAuditRecord>,
}

impl std::fmt::Debug for SandboxJobRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let guard = self.inner.lock().expect("sandbox registry mutex");
        f.debug_struct("SandboxJobRegistry")
            .field("job_count", &guard.jobs.len())
            .field("audit_count", &guard.audit.len())
            .finish()
    }
}

impl SandboxJobRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a running job and an audit stub; returns the public view.
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned.
    #[must_use]
    pub fn start_job(&self, job_type: &str, source_hash: &str, actor: &str) -> SandboxJobResponse {
        let id = new_job_id();
        let created_at_unix = unix_now();
        let response = SandboxJobResponse {
            id: id.clone(),
            job_type: job_type.to_owned(),
            status: SandboxJobStatus::Running,
            source_hash: source_hash.to_owned(),
            adjustments: None,
            proposal: None,
            error: None,
        };
        let audit = SandboxAuditRecord {
            job_id: id,
            actor: actor.to_owned(),
            job_type: job_type.to_owned(),
            source_hash: source_hash.to_owned(),
            status: "running".to_owned(),
            created_at_unix,
        };
        let mut guard = self.inner.lock().expect("sandbox registry mutex");
        guard.jobs.insert(
            response.id.clone(),
            JobRecord {
                response: response.clone(),
            },
        );
        guard.audit.push(audit);
        drop(guard);
        status_store::persist_job(&response);
        response
    }

    /// Updates job + matching audit row after a terminal quote/failure outcome.
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned.
    pub fn finish_job(
        &self,
        job_id: &str,
        status: SandboxJobStatus,
        adjustments: Option<Vec<Adjustment>>,
        error: Option<String>,
    ) {
        let mut guard = self.inner.lock().expect("sandbox registry mutex");
        if let Some(job) = guard.jobs.get_mut(job_id) {
            job.response.status = status;
            job.response.adjustments = adjustments.map(|rows| {
                rows.into_iter()
                    .map(SandboxAdjustmentResponse::from)
                    .collect()
            });
            job.response.error = error;
        }
        Self::touch_audit(&mut guard, job_id, status_label(status));
        let snapshot = guard.jobs.get(job_id).map(|job| job.response.clone());
        drop(guard);
        if let Some(response) = snapshot {
            status_store::persist_job(&response);
        }
    }

    /// Stores a validated proposal and moves the job to [`SandboxJobStatus::AwaitingCommit`].
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned.
    pub fn set_awaiting_commit(&self, job_id: &str, proposal: SandboxProposalResponse) {
        let mut guard = self.inner.lock().expect("sandbox registry mutex");
        if let Some(job) = guard.jobs.get_mut(job_id) {
            job.response.status = SandboxJobStatus::AwaitingCommit;
            job.response.proposal = Some(proposal);
            job.response.error = None;
        }
        Self::touch_audit(&mut guard, job_id, "awaiting_commit");
        let snapshot = guard.jobs.get(job_id).map(|job| job.response.clone());
        drop(guard);
        if let Some(response) = snapshot {
            status_store::persist_job(&response);
        }
    }

    /// Marks a proposal job as committed or discarded.
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned.
    pub fn finalize_proposal(&self, job_id: &str, status: SandboxJobStatus) {
        let mut guard = self.inner.lock().expect("sandbox registry mutex");
        if let Some(job) = guard.jobs.get_mut(job_id) {
            job.response.status = status;
        }
        Self::touch_audit(&mut guard, job_id, status_label(status));
        let snapshot = guard.jobs.get(job_id).map(|job| job.response.clone());
        drop(guard);
        if let Some(response) = snapshot {
            status_store::persist_job(&response);
        }
    }

    /// Test helper: awaiting commit with no proposal payload (covers commit guard).
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned.
    pub fn set_awaiting_commit_without_proposal(&self, job_id: &str) {
        let mut guard = self.inner.lock().expect("sandbox registry mutex");
        if let Some(job) = guard.jobs.get_mut(job_id) {
            job.response.status = SandboxJobStatus::AwaitingCommit;
            job.response.proposal = None;
        }
        Self::touch_audit(&mut guard, job_id, "awaiting_commit");
        drop(guard);
    }

    fn touch_audit(guard: &mut RegistryInner, job_id: &str, status_label: &str) {
        if let Some(row) = guard
            .audit
            .iter_mut()
            .rev()
            .find(|row| row.job_id == job_id)
        {
            status_label.clone_into(&mut row.status);
        }
    }

    /// Looks up a job by id.
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned.
    #[must_use]
    pub fn get(&self, job_id: &str) -> Option<SandboxJobResponse> {
        let memory = self
            .inner
            .lock()
            .expect("sandbox registry mutex")
            .jobs
            .get(job_id)
            .map(|job| job.response.clone());
        if let Some(job) = memory {
            return Some(job);
        }
        status_store::load_job(job_id)
    }

    /// Newest-first audit rows (capped).
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned.
    #[must_use]
    pub fn list_audit(&self, limit: usize) -> Vec<SandboxAuditRecord> {
        let guard = self.inner.lock().expect("sandbox registry mutex");
        guard.audit.iter().rev().take(limit).cloned().collect()
    }
}

const fn status_label(status: SandboxJobStatus) -> &'static str {
    match status {
        SandboxJobStatus::Running => "running",
        SandboxJobStatus::Succeeded => "succeeded",
        SandboxJobStatus::AwaitingCommit => "awaiting_commit",
        SandboxJobStatus::Committed => "committed",
        SandboxJobStatus::Discarded => "discarded",
        SandboxJobStatus::Failed => "failed",
    }
}

fn new_job_id() -> String {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).expect("getrandom");
    bytes
        .iter()
        .fold(String::with_capacity(32), |mut out, byte| {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs()
}

/// Stable fingerprint of guest source for audit.
#[must_use]
pub fn source_hash(source: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustashop_sandbox::Adjustment;

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
        registry.finalize_proposal(&job.id, SandboxJobStatus::Running);
        assert_eq!(registry.list_audit(1)[0].status, "running");
        registry.finalize_proposal(&job.id, SandboxJobStatus::AwaitingCommit);
        assert_eq!(registry.list_audit(1)[0].status, "awaiting_commit");
    }

    #[test]
    fn adjustment_from_domain() {
        let adj = Adjustment {
            label: "x".into(),
            amount_minor: 1,
            currency: "EUR".into(),
        };
        let row = SandboxAdjustmentResponse::from(adj);
        assert_eq!(row.label, "x");
        assert_eq!(row.amount_minor, 1);
    }
}
