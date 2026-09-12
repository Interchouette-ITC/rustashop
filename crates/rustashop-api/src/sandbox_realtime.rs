//! Sandbox job WebSocket hub and typed push events.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;

const CHANNEL_CAPACITY: usize = 64;

/// Proposal payload embedded in `job.proposal` events.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SandboxProposalEventBody {
    /// Domain event type (for example `cart.line_quantity_proposed`).
    pub event_type: String,
    /// Target cart id.
    pub cart_id: String,
    /// Legacy `id_product` mapped to rustashop variant id for v0.
    pub product_id: String,
    /// Proposed quantity operand.
    pub quantity: u32,
    /// Operator (`up` / `down` / `set`).
    pub operator: String,
}

/// Server-pushed sandbox job event (JSON over WebSocket).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SandboxJobEvent {
    /// Stable event name (`job.log` / `job.proposal` / `job.finished`).
    #[serde(rename = "type")]
    pub event_type: String,
    /// Protocol version.
    pub version: u32,
    /// Job id this event belongs to.
    pub job_id: String,
    /// Log line for `job.log`; empty otherwise.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
    /// Exit status string for `job.finished` (`ok` / `error` / `committed` / `discarded`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Validated guest proposal for `job.proposal`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal: Option<SandboxProposalEventBody>,
}

impl SandboxJobEvent {
    /// Builds a `job.log` v1 event.
    #[must_use]
    pub fn log(job_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            event_type: "job.log".to_owned(),
            version: 1,
            job_id: job_id.into(),
            message: message.into(),
            status: None,
            proposal: None,
        }
    }

    /// Builds a `job.proposal` v1 event (awaiting host commit).
    #[must_use]
    pub fn proposal(job_id: impl Into<String>, proposal: SandboxProposalEventBody) -> Self {
        Self {
            event_type: "job.proposal".to_owned(),
            version: 1,
            job_id: job_id.into(),
            message: String::new(),
            status: None,
            proposal: Some(proposal),
        }
    }

    /// Builds a `job.finished` v1 event.
    #[must_use]
    pub fn finished(job_id: impl Into<String>, status: impl Into<String>) -> Self {
        Self {
            event_type: "job.finished".to_owned(),
            version: 1,
            job_id: job_id.into(),
            message: String::new(),
            status: Some(status.into()),
            proposal: None,
        }
    }
}

/// In-process fan-out of sandbox job events by job id.
#[derive(Clone, Default)]
pub struct SandboxJobHub {
    rooms: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
}

impl std::fmt::Debug for SandboxJobHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rooms = self.rooms.lock().expect("sandbox hub mutex");
        f.debug_struct("SandboxJobHub")
            .field("room_count", &rooms.len())
            .finish()
    }
}

impl SandboxJobHub {
    /// Creates an empty hub.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes to push events for `job_id`.
    ///
    /// # Panics
    ///
    /// Panics if the hub mutex is poisoned.
    #[must_use]
    pub fn subscribe(&self, job_id: &str) -> broadcast::Receiver<String> {
        let mut rooms = self.rooms.lock().expect("sandbox hub mutex");
        if let Some(sender) = rooms.get(job_id) {
            return sender.subscribe();
        }
        let (sender, receiver) = broadcast::channel(CHANNEL_CAPACITY);
        rooms.insert(job_id.to_owned(), sender);
        receiver
    }

    /// Publishes a job event (no-op if nobody is subscribed).
    ///
    /// # Panics
    ///
    /// Panics if the hub mutex is poisoned or the event fails to serialize.
    pub fn publish(&self, event: &SandboxJobEvent) {
        let payload = serde_json::to_string(event).expect("sandbox job event serializes");
        let rooms = self.rooms.lock().expect("sandbox hub mutex");
        if let Some(sender) = rooms.get(&event.job_id) {
            let _ = sender.send(payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SandboxJobEvent, SandboxJobHub, SandboxProposalEventBody};

    #[test]
    fn publish_reaches_subscriber() {
        let hub = SandboxJobHub::new();
        assert!(format!("{hub:?}").contains("room_count"));
        let mut rx = hub.subscribe("job-1");
        let mut rx2 = hub.subscribe("job-1");
        hub.publish(&SandboxJobEvent::log("job-1", "hello"));
        let raw = rx.try_recv().expect("event");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(parsed["type"], "job.log");
        assert_eq!(parsed["message"], "hello");
        assert!(rx2.try_recv().is_ok());
        hub.publish(&SandboxJobEvent::log("nobody", "ignored"));
    }

    #[test]
    fn finished_event_includes_status() {
        let event = SandboxJobEvent::finished("j", "ok");
        let raw = serde_json::to_string(&event).expect("json");
        assert!(raw.contains("job.finished"));
        assert!(raw.contains("\"ok\""));
    }

    #[test]
    fn proposal_event_includes_body() {
        let event = SandboxJobEvent::proposal(
            "j",
            SandboxProposalEventBody {
                event_type: "cart.line_quantity_proposed".into(),
                cart_id: "c1".into(),
                product_id: "v1".into(),
                quantity: 2,
                operator: "set".into(),
            },
        );
        let raw = serde_json::to_string(&event).expect("json");
        assert!(raw.contains("job.proposal"));
        assert!(raw.contains("cart.line_quantity_proposed"));
    }
}
