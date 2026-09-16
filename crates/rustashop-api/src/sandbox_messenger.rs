//! Sandbox job enqueue / consume via `serenade-messenger`.
//!
//! Default: [`InMemoryTransport`] + in-process worker loop (CI / local).
//! Feature `messenger-redis`: durable [`WireEnvelope`] queue on Redis.

use std::time::Duration;

use rustashop_sandbox::{CartSnapshot, LegacyHookInput};
use serenade_messenger::{Command, Envelope, InMemoryTransport, Message, Transport};
use tracing::{info, warn};

use crate::sandbox_autonomous::run_cart_quantity_job;
use crate::sandbox_jobs::{SandboxJobRegistry, run_quote_job};
use crate::sandbox_realtime::SandboxJobHub;

/// Env URL for durable Redis messenger (`redis://…`).
pub const MESSENGER_REDIS_URL_ENV: &str = "RUSTASHOP_MESSENGER_REDIS_URL";
/// Env Redis list key (default `rustashop:sandbox:jobs`).
pub const MESSENGER_REDIS_LIST_ENV: &str = "RUSTASHOP_MESSENGER_REDIS_LIST";
/// Default Redis list key when [`MESSENGER_REDIS_LIST_ENV`] is unset.
pub const DEFAULT_MESSENGER_REDIS_LIST: &str = "rustashop:sandbox:jobs";
/// Wire / in-process message name for sandbox job work.
pub const SANDBOX_JOB_MESSAGE: &str = "rustashop.sandbox.job";

/// Work unit enqueued for the sandbox worker.
#[derive(Debug, Clone)]
pub enum SandboxJobWork {
    /// Python quote guest.
    Quote {
        /// Job id in the registry.
        job_id: String,
        /// Cart snapshot for the guest.
        cart: CartSnapshot,
        /// Guest source text.
        source: String,
    },
    /// PHP cart-quantity migration guest.
    CartQuantity {
        /// Job id in the registry.
        job_id: String,
        /// Legacy hook input.
        input: LegacyHookInput,
        /// Guest source text.
        source: String,
    },
}

impl Message for SandboxJobWork {
    const NAME: &'static str = SANDBOX_JOB_MESSAGE;
}

impl Command for SandboxJobWork {}

/// In-memory sandbox job transport + worker handle factory.
#[derive(Clone)]
pub struct SandboxJobMessenger {
    transport: InMemoryTransport,
}

impl std::fmt::Debug for SandboxJobMessenger {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SandboxJobMessenger")
            .field("queued", &self.transport.len())
            .finish()
    }
}

impl SandboxJobMessenger {
    /// Creates an empty in-memory messenger.
    #[must_use]
    pub fn new() -> Self {
        Self {
            transport: InMemoryTransport::new(),
        }
    }

    /// Shared in-memory transport (tests / drain helpers).
    #[must_use]
    pub const fn transport(&self) -> &InMemoryTransport {
        &self.transport
    }

    /// Enqueues work for a later worker consume.
    ///
    /// # Errors
    ///
    /// Returns a messenger transport error when send fails.
    pub async fn enqueue(
        &self,
        work: SandboxJobWork,
    ) -> Result<(), serenade_messenger::MessengerError> {
        self.transport.send(Envelope::new(work)).await
    }

    /// Pops and runs one job if the queue is non-empty. Returns whether work ran.
    pub async fn drain_one(&self, registry: &SandboxJobRegistry, hub: &SandboxJobHub) -> bool {
        let Some(envelope) = self.transport.pop() else {
            return false;
        };
        dispatch_envelope(envelope, registry, hub).await;
        true
    }

    /// Spawns a background loop that drains the in-memory queue.
    #[must_use]
    pub fn spawn_worker(
        &self,
        registry: SandboxJobRegistry,
        hub: SandboxJobHub,
    ) -> tokio::task::JoinHandle<()> {
        let transport = self.transport.clone();
        tokio::spawn(async move {
            loop {
                if let Some(envelope) = transport.pop() {
                    dispatch_envelope(envelope, &registry, &hub).await;
                } else {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }
        })
    }
}

impl Default for SandboxJobMessenger {
    fn default() -> Self {
        Self::new()
    }
}

async fn dispatch_envelope(envelope: Envelope, registry: &SandboxJobRegistry, hub: &SandboxJobHub) {
    let Some(work) = envelope.downcast_ref::<SandboxJobWork>() else {
        warn!(
            message_name = envelope.message_name(),
            "sandbox messenger: unknown envelope type"
        );
        return;
    };
    match work.clone() {
        SandboxJobWork::Quote {
            job_id,
            cart,
            source,
        } => run_quote_job(registry, hub, &job_id, &cart, &source).await,
        SandboxJobWork::CartQuantity {
            job_id,
            input,
            source,
        } => run_cart_quantity_job(registry, hub, &job_id, &input, &source).await,
    }
}

/// JSON payload for Redis [`WireEnvelope`] frames (feature `messenger-redis`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SandboxJobWirePayload {
    /// Python quote guest.
    Quote {
        /// Job id.
        job_id: String,
        /// Cart snapshot.
        cart: CartSnapshot,
        /// Guest source.
        source: String,
    },
    /// PHP cart-quantity guest.
    CartQuantity {
        /// Job id.
        job_id: String,
        /// Legacy hook input.
        input: LegacyHookInput,
        /// Guest source.
        source: String,
    },
}

impl From<SandboxJobWork> for SandboxJobWirePayload {
    fn from(work: SandboxJobWork) -> Self {
        match work {
            SandboxJobWork::Quote {
                job_id,
                cart,
                source,
            } => Self::Quote {
                job_id,
                cart,
                source,
            },
            SandboxJobWork::CartQuantity {
                job_id,
                input,
                source,
            } => Self::CartQuantity {
                job_id,
                input,
                source,
            },
        }
    }
}

impl From<SandboxJobWirePayload> for SandboxJobWork {
    fn from(payload: SandboxJobWirePayload) -> Self {
        match payload {
            SandboxJobWirePayload::Quote {
                job_id,
                cart,
                source,
            } => Self::Quote {
                job_id,
                cart,
                source,
            },
            SandboxJobWirePayload::CartQuantity {
                job_id,
                input,
                source,
            } => Self::CartQuantity {
                job_id,
                input,
                source,
            },
        }
    }
}

/// Starts the default in-memory worker, or a Redis consumer when configured.
#[must_use]
pub fn spawn_configured_worker(
    messenger: &SandboxJobMessenger,
    registry: SandboxJobRegistry,
    hub: SandboxJobHub,
) -> tokio::task::JoinHandle<()> {
    #[cfg(feature = "messenger-redis")]
    {
        if let Ok(url) = std::env::var(MESSENGER_REDIS_URL_ENV) {
            let list = std::env::var(MESSENGER_REDIS_LIST_ENV)
                .unwrap_or_else(|_| DEFAULT_MESSENGER_REDIS_LIST.to_owned());
            info!(%url, %list, "sandbox messenger: Redis worker");
            return spawn_redis_worker(url, list, registry, hub);
        }
    }
    info!("sandbox messenger: in-memory worker");
    messenger.spawn_worker(registry, hub)
}

/// Enqueues on Redis when `messenger-redis` + URL are set; otherwise in-memory.
pub async fn enqueue_sandbox_job(
    messenger: &SandboxJobMessenger,
    work: SandboxJobWork,
) -> Result<(), String> {
    #[cfg(feature = "messenger-redis")]
    {
        if let Ok(url) = std::env::var(MESSENGER_REDIS_URL_ENV) {
            let list = std::env::var(MESSENGER_REDIS_LIST_ENV)
                .unwrap_or_else(|_| DEFAULT_MESSENGER_REDIS_LIST.to_owned());
            return enqueue_redis(&url, &list, work)
                .await
                .map_err(|error| error.to_string());
        }
    }
    messenger
        .enqueue(work)
        .await
        .map_err(|error| error.to_string())
}

#[cfg(feature = "messenger-redis")]
fn spawn_redis_worker(
    url: String,
    list: String,
    registry: SandboxJobRegistry,
    hub: SandboxJobHub,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use std::sync::Arc;
        use tracing::error;

        let config = serenade_messenger::RedisTransportConfig::new(url).with_list_key(list);
        let transport = match serenade_messenger::RedisTransport::connect(config) {
            Ok(transport) => Arc::new(transport),
            Err(error) => {
                error!(%error, "sandbox messenger: Redis connect failed");
                return;
            }
        };
        loop {
            match transport.receive_wire().await {
                Ok(Some(frame)) => match decode_wire_payload(frame.payload()) {
                    Ok(work) => {
                        dispatch_work(work, &registry, &hub).await;
                    }
                    Err(error) => warn!(%error, "sandbox messenger: bad Redis payload"),
                },
                Ok(None) => tokio::time::sleep(Duration::from_millis(25)).await,
                Err(error) => {
                    error!(%error, "sandbox messenger: Redis receive failed");
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    })
}

#[cfg(feature = "messenger-redis")]
async fn enqueue_redis(
    url: &str,
    list: &str,
    work: SandboxJobWork,
) -> Result<(), serenade_messenger::MessengerError> {
    let config = serenade_messenger::RedisTransportConfig::new(url).with_list_key(list);
    let transport = serenade_messenger::RedisTransport::connect(config)?;
    let bytes = serde_json::to_vec(&SandboxJobWirePayload::from(work)).map_err(|error| {
        serenade_messenger::MessengerError::Transport {
            message: error.to_string(),
        }
    })?;
    let frame = serenade_messenger::WireEnvelope::new(SANDBOX_JOB_MESSAGE, bytes)?;
    transport.send_wire(frame).await
}

#[cfg(feature = "messenger-redis")]
fn decode_wire_payload(bytes: &[u8]) -> Result<SandboxJobWork, String> {
    serde_json::from_slice::<SandboxJobWirePayload>(bytes)
        .map(SandboxJobWork::from)
        .map_err(|error| error.to_string())
}

#[cfg(feature = "messenger-redis")]
async fn dispatch_work(work: SandboxJobWork, registry: &SandboxJobRegistry, hub: &SandboxJobHub) {
    match work {
        SandboxJobWork::Quote {
            job_id,
            cart,
            source,
        } => run_quote_job(registry, hub, &job_id, &cart, &source).await,
        SandboxJobWork::CartQuantity {
            job_id,
            input,
            source,
        } => run_cart_quantity_job(registry, hub, &job_id, &input, &source).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustashop_sandbox::{CartLine, Money};

    #[tokio::test]
    async fn in_memory_enqueue_and_pop() {
        let messenger = SandboxJobMessenger::new();
        let cart = CartSnapshot {
            currency: "EUR".into(),
            lines: vec![CartLine {
                sku: "SKU".into(),
                quantity: 1,
                unit_price: Money {
                    amount_minor: 100,
                    currency: "EUR".into(),
                },
            }],
        };
        messenger
            .enqueue(SandboxJobWork::Quote {
                job_id: "job-1".into(),
                cart,
                source: "print([])".into(),
            })
            .await
            .expect("enqueue");
        assert_eq!(messenger.transport().len(), 1);
        let envelope = messenger.transport().pop().expect("queued");
        assert_eq!(envelope.message_name(), SANDBOX_JOB_MESSAGE);
        match envelope.downcast_ref::<SandboxJobWork>().expect("type") {
            SandboxJobWork::Quote { job_id, .. } => assert_eq!(job_id, "job-1"),
            SandboxJobWork::CartQuantity { .. } => panic!("expected quote work"),
        }
        assert!(messenger.transport().is_empty());
    }
}
