//! Shared sandbox job queue: registry, messenger, and Wasmer runners.
//!
//! The commerce API enqueues work. The worker process (or an inline API worker
//! for single-process tests) consumes and updates job status.

mod hub;
mod messenger;
mod policy;
mod registry;
mod runner;
mod status_store;

pub use hub::{SandboxJobEvent, SandboxJobHub, SandboxProposalEventBody};
pub use messenger::{
    ConsumeOptions, DEFAULT_MESSENGER_REDIS_LIST, MESSENGER_REDIS_LIST_ENV,
    MESSENGER_REDIS_URL_ENV, SANDBOX_JOB_MESSAGE, SandboxJobMessenger, SandboxJobWirePayload,
    SandboxJobWork, enqueue_sandbox_job, run_consume_loop, spawn_configured_worker,
};
pub use policy::{INLINE_WORKER_ENV, should_spawn_inline_worker};
pub use registry::{
    CreateSandboxJobRequest, JOB_TYPE_CART_QUANTITY, JOB_TYPE_QUOTE, SandboxAdjustmentResponse,
    SandboxAuditRecord, SandboxJobLine, SandboxJobRegistry, SandboxJobResponse, SandboxJobStatus,
    SandboxProposalResponse, source_hash,
};
pub use runner::{run_cart_quantity_job, run_quote_job};

pub use messenger::force_enqueue_failure;

#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    static MESSENGER_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Serializes tests that mutate messenger Redis / inline-worker env vars.
    pub fn lock() -> MutexGuard<'static, ()> {
        MESSENGER_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
