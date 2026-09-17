//! Sandbox job enqueue / consume via `serenade-messenger`.
//!
//! Default: [`InMemoryTransport`] + in-process worker loop (CI / local).
//! Feature `messenger-redis`: durable Redis wire-envelope queue.

use std::time::Duration;

use rustashop_sandbox::{CartSnapshot, LegacyHookInput};
use serenade_messenger::{Command, Envelope, InMemoryTransport, Message, Transport};
use tracing::{info, warn};

use crate::hub::SandboxJobHub;
use crate::registry::SandboxJobRegistry;
use crate::runner::{run_cart_quantity_job, run_quote_job};

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
        let message_name = envelope.message_name();
        warn!(message_name, "sandbox messenger: unknown envelope type");
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

/// JSON payload for Redis wire-envelope frames (feature `messenger-redis`).
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

/// Options for [`run_consume_loop`] (console `messenger:consume`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ConsumeOptions {
    /// Process at most one message (or empty poll) then exit.
    pub once: bool,
    /// Cap on messages processed (`None` = unlimited unless `once`).
    pub limit: Option<usize>,
}

/// Runs a foreground consume loop for the console worker.
///
/// With Redis URL + feature `redis`, consumes wire frames. Otherwise drains the
/// in-memory messenger (same-process / `--once` dogfood).
///
/// # Errors
///
/// Returns a string when Redis connect fails.
pub async fn run_consume_loop(
    messenger: &SandboxJobMessenger,
    registry: SandboxJobRegistry,
    hub: SandboxJobHub,
    options: ConsumeOptions,
) -> Result<usize, String> {
    let max = if options.once {
        1
    } else {
        options.limit.unwrap_or(usize::MAX)
    };
    #[cfg(feature = "redis")]
    {
        if let Ok(url) = std::env::var(MESSENGER_REDIS_URL_ENV) {
            let list = std::env::var(MESSENGER_REDIS_LIST_ENV)
                .unwrap_or_else(|_| DEFAULT_MESSENGER_REDIS_LIST.to_owned());
            return consume_redis_loop(url, list, registry, hub, max, options.once).await;
        }
    }
    let mut processed = 0_usize;
    let mut polls = 0_usize;
    while processed < max {
        if messenger.drain_one(&registry, &hub).await {
            processed += 1;
            continue;
        }
        if options.once || options.limit.is_some() {
            polls += 1;
            if options.once || polls >= max {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Ok(processed)
}

#[cfg(feature = "redis")]
async fn consume_redis_loop(
    url: String,
    list: String,
    registry: SandboxJobRegistry,
    hub: SandboxJobHub,
    max: usize,
    once: bool,
) -> Result<usize, String> {
    use std::sync::Arc;
    let config = serenade_messenger::RedisTransportConfig::new(url).with_list_key(list);
    let transport = Arc::new(
        serenade_messenger::RedisTransport::connect(config).map_err(|error| error.to_string())?,
    );
    let mut processed = 0_usize;
    let mut empty_polls = 0_usize;
    while processed < max {
        match transport.receive_wire().await {
            Ok(Some(frame)) => match decode_wire_payload(frame.payload()) {
                Ok(work) => {
                    dispatch_work(work, &registry, &hub).await;
                    processed += 1;
                    empty_polls = 0;
                }
                Err(error) => warn!(%error, "sandbox messenger: bad Redis payload"),
            },
            Ok(None) => {
                if once || (max != usize::MAX && empty_polls + 1 >= max && processed == 0) {
                    break;
                }
                empty_polls += 1;
                if max != usize::MAX && processed + empty_polls >= max {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(processed)
}

/// Starts the default in-memory worker, or a Redis consumer when configured.
#[must_use]
pub fn spawn_configured_worker(
    messenger: &SandboxJobMessenger,
    registry: SandboxJobRegistry,
    hub: SandboxJobHub,
) -> tokio::task::JoinHandle<()> {
    #[cfg(feature = "redis")]
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
///
/// # Errors
///
/// Returns a string when the transport send fails, or when tests force a failure.
pub async fn enqueue_sandbox_job(
    messenger: &SandboxJobMessenger,
    work: SandboxJobWork,
) -> Result<(), String> {
    if FAIL_ENQUEUE.with(std::cell::Cell::get) {
        return Err("forced enqueue failure".to_owned());
    }
    #[cfg(feature = "redis")]
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

thread_local! {
    static FAIL_ENQUEUE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test helper: force the next [`enqueue_sandbox_job`] calls to fail.
pub fn force_enqueue_failure(fail: bool) {
    FAIL_ENQUEUE.with(|cell| cell.set(fail));
}

#[cfg(feature = "redis")]
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
                other => {
                    if let Err(error) = other {
                        error!(%error, "sandbox messenger: Redis receive failed");
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        }
    })
}

#[cfg(feature = "redis")]
async fn enqueue_redis(
    url: &str,
    list: &str,
    work: SandboxJobWork,
) -> Result<(), serenade_messenger::MessengerError> {
    let config = serenade_messenger::RedisTransportConfig::new(url).with_list_key(list);
    let transport = serenade_messenger::RedisTransport::connect(config)?;
    let bytes = serde_json::to_vec(&SandboxJobWirePayload::from(work))
        .expect("sandbox job wire payload serializes");
    let frame = serenade_messenger::WireEnvelope::new(SANDBOX_JOB_MESSAGE, bytes)?;
    transport.send_wire(frame).await
}

#[cfg(feature = "redis")]
fn decode_wire_payload(bytes: &[u8]) -> Result<SandboxJobWork, String> {
    serde_json::from_slice::<SandboxJobWirePayload>(bytes)
        .map(SandboxJobWork::from)
        .map_err(|error| error.to_string())
}

#[cfg(feature = "redis")]
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
#[allow(clippy::await_holding_lock)] // std::Mutex serializes env mutation across async tests
mod tests {
    use super::*;
    use rustashop_sandbox::{CART_UPDATE_QUANTITY_HOOK, CartLine, Money};
    use serenade_messenger::{Command, Message};

    fn sample_cart() -> CartSnapshot {
        CartSnapshot {
            currency: "EUR".into(),
            lines: vec![CartLine {
                sku: "SKU".into(),
                quantity: 1,
                unit_price: Money {
                    amount_minor: 100,
                    currency: "EUR".into(),
                },
            }],
        }
    }

    fn clear_redis_env() {
        unsafe {
            std::env::remove_var(MESSENGER_REDIS_URL_ENV);
            std::env::remove_var(MESSENGER_REDIS_LIST_ENV);
        }
    }

    #[test]
    fn debug_default_and_message_name() {
        let messenger = SandboxJobMessenger::default();
        assert!(format!("{messenger:?}").contains("SandboxJobMessenger"));
        assert_eq!(SandboxJobWork::NAME, SANDBOX_JOB_MESSAGE);
        let _ = SandboxJobWork::Quote {
            job_id: "x".into(),
            cart: sample_cart(),
            source: String::new(),
        };
    }

    #[tokio::test]
    async fn in_memory_enqueue_and_pop() {
        let messenger = SandboxJobMessenger::new();
        messenger
            .enqueue(SandboxJobWork::Quote {
                job_id: "job-1".into(),
                cart: sample_cart(),
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

    #[tokio::test]
    async fn drain_one_empty_and_unknown_envelope() {
        #[derive(Debug)]
        struct OtherCmd;
        impl Message for OtherCmd {
            const NAME: &'static str = "other";
        }
        impl Command for OtherCmd {}

        let messenger = SandboxJobMessenger::new();
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        assert!(!messenger.drain_one(&registry, &hub).await);

        messenger
            .transport()
            .send(Envelope::new(OtherCmd))
            .await
            .expect("send");
        assert!(messenger.drain_one(&registry, &hub).await);
    }

    #[tokio::test]
    async fn enqueue_sandbox_job_and_wire_payload_roundtrip() {
        let _g = crate::test_env::lock();
        clear_redis_env();
        force_enqueue_failure(false);
        let messenger = SandboxJobMessenger::new();
        let quote = SandboxJobWork::Quote {
            job_id: "q1".into(),
            cart: sample_cart(),
            source: "src".into(),
        };
        enqueue_sandbox_job(&messenger, quote.clone())
            .await
            .expect("enqueue");
        assert_eq!(messenger.transport().len(), 1);

        let cart_qty = SandboxJobWork::CartQuantity {
            job_id: "c1".into(),
            input: LegacyHookInput {
                hook: CART_UPDATE_QUANTITY_HOOK.into(),
                cart_id: "cart".into(),
                id_product: "v1".into(),
                quantity: 2,
                operator: "set".into(),
            },
            source: "php".into(),
        };
        let wire = SandboxJobWirePayload::from(cart_qty);
        assert!(matches!(
            SandboxJobWork::from(wire),
            SandboxJobWork::CartQuantity { job_id, .. } if job_id == "c1"
        ));
        let wire_quote = SandboxJobWirePayload::from(quote);
        assert!(matches!(
            SandboxJobWork::from(wire_quote),
            SandboxJobWork::Quote { .. }
        ));
    }

    #[tokio::test]
    async fn force_enqueue_failure_flag() {
        let _g = crate::test_env::lock();
        clear_redis_env();
        force_enqueue_failure(true);
        let messenger = SandboxJobMessenger::new();
        let err = enqueue_sandbox_job(
            &messenger,
            SandboxJobWork::Quote {
                job_id: "x".into(),
                cart: sample_cart(),
                source: String::new(),
            },
        )
        .await
        .expect_err("forced");
        assert!(err.contains("forced"));
        force_enqueue_failure(false);
    }

    #[tokio::test]
    async fn spawn_configured_worker_drains_quote() {
        let _g = crate::test_env::lock();
        clear_redis_env();
        let messenger = SandboxJobMessenger::new();
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let job = registry.start_job("quote", "hash", "test");
        messenger
            .enqueue(SandboxJobWork::Quote {
                job_id: job.id.clone(),
                cart: sample_cart(),
                source: "not-valid-python".into(),
            })
            .await
            .expect("enqueue");
        let handle = spawn_configured_worker(&messenger, registry.clone(), hub);
        let finished = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let status = registry.get(&job.id).expect("job").status;
                if status != crate::registry::SandboxJobStatus::Running {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        handle.abort();
        let status = finished.expect("worker did not finish job");
        assert_ne!(status, crate::registry::SandboxJobStatus::Running);
        assert!(messenger.transport().is_empty());
    }

    #[tokio::test]
    async fn run_consume_loop_limit_drains_and_stops() {
        let _g = crate::test_env::lock();
        clear_redis_env();
        let messenger = SandboxJobMessenger::new();
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let job = registry.start_job("quote", "hash", "test");
        messenger
            .enqueue(SandboxJobWork::Quote {
                job_id: job.id.clone(),
                cart: sample_cart(),
                source: "not-valid-python".into(),
            })
            .await
            .expect("enqueue");
        let processed = run_consume_loop(
            &messenger,
            registry.clone(),
            hub,
            ConsumeOptions {
                once: false,
                limit: Some(2),
            },
        )
        .await
        .expect("consume");
        assert_eq!(processed, 1);
        assert_ne!(
            registry.get(&job.id).expect("job").status,
            crate::registry::SandboxJobStatus::Running
        );
    }

    #[cfg(feature = "redis")]
    #[test]
    fn decode_wire_payload_ok_and_err() {
        let quote = SandboxJobWork::Quote {
            job_id: "q".into(),
            cart: sample_cart(),
            source: "src".into(),
        };
        let bytes = serde_json::to_vec(&SandboxJobWirePayload::from(quote)).expect("json");
        let decoded = decode_wire_payload(&bytes).expect("decode");
        assert!(matches!(decoded, SandboxJobWork::Quote { job_id, .. } if job_id == "q"));

        let cart_qty = SandboxJobWork::CartQuantity {
            job_id: "c".into(),
            input: LegacyHookInput {
                hook: CART_UPDATE_QUANTITY_HOOK.into(),
                cart_id: "cart".into(),
                id_product: "v1".into(),
                quantity: 2,
                operator: "set".into(),
            },
            source: "php".into(),
        };
        let bytes = serde_json::to_vec(&SandboxJobWirePayload::from(cart_qty)).expect("json");
        let decoded = decode_wire_payload(&bytes).expect("decode");
        assert!(matches!(decoded, SandboxJobWork::CartQuantity { job_id, .. } if job_id == "c"));
        assert!(decode_wire_payload(b"not-json").is_err());
    }

    #[cfg(feature = "redis")]
    #[tokio::test]
    async fn redis_connect_failures_are_surfaced() {
        let _g = crate::test_env::lock();
        unsafe {
            std::env::set_var(MESSENGER_REDIS_URL_ENV, "redis://127.0.0.1:1");
            std::env::remove_var(MESSENGER_REDIS_LIST_ENV);
        }
        let messenger = SandboxJobMessenger::new();
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let err = run_consume_loop(
            &messenger,
            registry.clone(),
            hub.clone(),
            ConsumeOptions {
                once: true,
                limit: None,
            },
        )
        .await
        .expect_err("bad redis url");
        assert_ne!(err, "");
        let enqueue_err = enqueue_sandbox_job(
            &messenger,
            SandboxJobWork::Quote {
                job_id: "x".into(),
                cart: sample_cart(),
                source: String::new(),
            },
        )
        .await
        .expect_err("enqueue redis");
        assert_ne!(enqueue_err, "");
        let handle = spawn_configured_worker(&messenger, registry, hub);
        tokio::time::sleep(Duration::from_millis(150)).await;
        handle.abort();
        unsafe {
            std::env::remove_var(MESSENGER_REDIS_URL_ENV);
        }
    }

    #[cfg(feature = "redis")]
    fn redis_up(url: &str) -> bool {
        redis::Client::open(url)
            .ok()
            .and_then(|client| client.get_connection().ok())
            .is_some()
    }

    #[cfg(feature = "redis")]
    fn bind_redis_env(url: &str, list: &str) {
        unsafe {
            std::env::set_var(MESSENGER_REDIS_URL_ENV, url);
            std::env::set_var(MESSENGER_REDIS_LIST_ENV, list);
        }
    }

    #[cfg(feature = "redis")]
    async fn push_wire(url: &str, list: &str, payload: &[u8]) {
        let config = serenade_messenger::RedisTransportConfig::new(url).with_list_key(list);
        let transport =
            serenade_messenger::RedisTransport::connect(config).expect("redis transport");
        let frame =
            serenade_messenger::WireEnvelope::new(SANDBOX_JOB_MESSAGE, payload).expect("wire");
        transport.send_wire(frame).await.expect("push");
    }

    #[cfg(feature = "redis")]
    fn push_raw(url: &str, list: &str, payload: &[u8]) {
        use redis::Commands;
        let client = redis::Client::open(url).expect("client");
        let mut conn = client.get_connection().expect("conn");
        let _: () = conn.rpush(list, payload).expect("push");
    }

    #[cfg(feature = "redis")]
    #[tokio::test]
    async fn redis_enqueue_and_consume_quote() {
        let _g = crate::test_env::lock();
        let url = "redis://127.0.0.1:6379/0";
        assert!(
            redis_up(url),
            "redis must be listening at {url} (CI service or local docker)"
        );
        let list = format!("rustashop:sandbox:jobs:quote:{}", std::process::id());
        bind_redis_env(url, &list);
        let messenger = SandboxJobMessenger::new();
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let job = registry.start_job("quote", "hash", "redis-test");
        enqueue_sandbox_job(
            &messenger,
            SandboxJobWork::Quote {
                job_id: job.id.clone(),
                cart: sample_cart(),
                source: "not-valid-python".into(),
            },
        )
        .await
        .expect("enqueue redis");
        let processed = run_consume_loop(
            &messenger,
            registry.clone(),
            hub,
            ConsumeOptions {
                once: false,
                limit: Some(5),
            },
        )
        .await
        .expect("consume redis");
        assert!(processed >= 1);
        assert_ne!(
            registry.get(&job.id).expect("job").status,
            crate::registry::SandboxJobStatus::Running
        );
        clear_redis_env();
    }

    #[cfg(feature = "redis")]
    #[tokio::test]
    async fn redis_bad_json_and_corrupt_frame() {
        let _g = crate::test_env::lock();
        let url = "redis://127.0.0.1:6379/0";
        assert!(redis_up(url), "redis must be listening at {url}");
        let list = format!("rustashop:sandbox:jobs:bad:{}", std::process::id());
        bind_redis_env(url, &list);
        let messenger = SandboxJobMessenger::new();
        push_wire(url, &list, b"not-json").await;
        let skipped = run_consume_loop(
            &messenger,
            SandboxJobRegistry::new(),
            SandboxJobHub::new(),
            ConsumeOptions {
                once: true,
                limit: None,
            },
        )
        .await
        .expect("consume junk");
        assert_eq!(skipped, 0);
        push_raw(url, &list, b"not-a-wire-envelope");
        assert!(
            run_consume_loop(
                &messenger,
                SandboxJobRegistry::new(),
                SandboxJobHub::new(),
                ConsumeOptions {
                    once: true,
                    limit: None,
                },
            )
            .await
            .is_err(),
            "corrupt wire frame should error"
        );
        clear_redis_env();
    }

    #[cfg(feature = "redis")]
    #[tokio::test]
    async fn redis_spawn_worker_drains_and_handles_errors() {
        let _g = crate::test_env::lock();
        let url = "redis://127.0.0.1:6379/0";
        assert!(redis_up(url), "redis must be listening at {url}");
        let list = format!("rustashop:sandbox:jobs:worker:{}", std::process::id());
        bind_redis_env(url, &list);
        let messenger = SandboxJobMessenger::new();
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let job = registry.start_job("quote", "hash", "redis-worker");
        enqueue_sandbox_job(
            &messenger,
            SandboxJobWork::Quote {
                job_id: job.id.clone(),
                cart: sample_cart(),
                source: "not-valid-python".into(),
            },
        )
        .await
        .expect("enqueue for worker");
        push_wire(url, &list, b"{}").await;
        let handle = spawn_configured_worker(&messenger, registry.clone(), hub.clone());
        let finished = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let status = registry.get(&job.id).expect("job").status;
                if status != crate::registry::SandboxJobStatus::Running {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        handle.abort();
        assert_ne!(
            finished.expect("redis worker timeout"),
            crate::registry::SandboxJobStatus::Running
        );
        let handle = spawn_configured_worker(&messenger, registry.clone(), hub);
        tokio::time::sleep(Duration::from_millis(60)).await;
        handle.abort();
        push_raw(url, &list, b"not-a-wire-envelope");
        let handle = spawn_configured_worker(&messenger, registry, SandboxJobHub::new());
        tokio::time::sleep(Duration::from_millis(100)).await;
        handle.abort();
        clear_redis_env();
    }

    #[cfg(feature = "redis")]
    #[tokio::test]
    async fn redis_consume_cart_quantity() {
        let _g = crate::test_env::lock();
        let url = "redis://127.0.0.1:6379/0";
        assert!(redis_up(url), "redis must be listening at {url}");
        let list = format!("rustashop:sandbox:jobs:cart:{}", std::process::id());
        bind_redis_env(url, &list);
        let messenger = SandboxJobMessenger::new();
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let job = registry.start_job("cart_quantity", "hash", "redis-cart");
        enqueue_sandbox_job(
            &messenger,
            SandboxJobWork::CartQuantity {
                job_id: job.id.clone(),
                input: LegacyHookInput {
                    hook: CART_UPDATE_QUANTITY_HOOK.into(),
                    cart_id: "cart-redis".into(),
                    id_product: "v1".into(),
                    quantity: 1,
                    operator: "set".into(),
                },
                source: rustashop_sandbox::php_migration_hook_source(),
            },
        )
        .await
        .expect("enqueue cart qty");
        let _wasmer = rustashop_sandbox::WASMER_TEST_GATE.lock().await;
        let n = run_consume_loop(
            &messenger,
            registry,
            hub,
            ConsumeOptions {
                once: false,
                limit: Some(3),
            },
        )
        .await
        .expect("consume cart");
        assert!(n >= 1);
        clear_redis_env();
    }

    #[tokio::test]
    async fn run_consume_loop_unlimited_polls_until_timeout() {
        let _g = crate::test_env::lock();
        clear_redis_env();
        let messenger = SandboxJobMessenger::new();
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let run = run_consume_loop(
            &messenger,
            registry,
            hub,
            ConsumeOptions {
                once: false,
                limit: None,
            },
        );
        let timed_out = tokio::time::timeout(Duration::from_millis(40), run).await;
        assert!(
            timed_out.is_err(),
            "unlimited empty consume should keep polling"
        );
    }

    #[tokio::test]
    async fn drain_one_runs_quote_job() {
        let messenger = SandboxJobMessenger::new();
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let job = registry.start_job("quote", "hash", "test");
        messenger
            .enqueue(SandboxJobWork::Quote {
                job_id: job.id.clone(),
                cart: sample_cart(),
                source: "not-valid-python".into(),
            })
            .await
            .expect("enqueue");
        assert!(messenger.drain_one(&registry, &hub).await);
        let done = registry.get(&job.id).expect("job");
        assert_ne!(done.status, crate::registry::SandboxJobStatus::Running);
    }

    #[tokio::test]
    async fn drain_one_runs_cart_quantity_job() {
        let _wasmer = rustashop_sandbox::WASMER_TEST_GATE.lock().await;
        let messenger = SandboxJobMessenger::new();
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let job = registry.start_job("cart_quantity", "hash", "test");
        messenger
            .enqueue(SandboxJobWork::CartQuantity {
                job_id: job.id.clone(),
                input: LegacyHookInput {
                    hook: CART_UPDATE_QUANTITY_HOOK.into(),
                    cart_id: "cart-drain".into(),
                    id_product: "variant-1".into(),
                    quantity: 1,
                    operator: "set".into(),
                },
                source: rustashop_sandbox::php_migration_hook_source(),
            })
            .await
            .expect("enqueue");
        assert!(messenger.drain_one(&registry, &hub).await);
        let done = registry.get(&job.id).expect("job");
        assert_eq!(
            done.status,
            crate::registry::SandboxJobStatus::AwaitingCommit
        );
    }
}
