//! Cart WebSocket hub and typed push events.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;

use crate::carts::CartResponse;

const CHANNEL_CAPACITY: usize = 64;

/// Server-pushed cart event (JSON over WebSocket).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CartRealtimeEvent {
    /// Stable event name (`cart.updated`).
    #[serde(rename = "type")]
    pub event_type: String,
    /// Protocol version.
    pub version: u32,
    /// Full cart snapshot after the mutation.
    pub cart: CartResponse,
}

impl CartRealtimeEvent {
    /// Builds a `cart.updated` v1 event.
    #[must_use]
    pub fn updated(cart: CartResponse) -> Self {
        Self {
            event_type: "cart.updated".to_owned(),
            version: 1,
            cart,
        }
    }
}

/// In-process fan-out of cart events by cart id.
#[derive(Clone, Default)]
pub struct CartHub {
    rooms: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
}

impl std::fmt::Debug for CartHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rooms = self.rooms.lock().expect("cart hub mutex");
        f.debug_struct("CartHub")
            .field("room_count", &rooms.len())
            .finish()
    }
}

impl CartHub {
    /// Creates an empty hub.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes to push events for `cart_id`.
    ///
    /// # Panics
    ///
    /// Panics if the hub mutex is poisoned.
    #[must_use]
    pub fn subscribe(&self, cart_id: &str) -> broadcast::Receiver<String> {
        let mut rooms = self.rooms.lock().expect("cart hub mutex");
        if let Some(sender) = rooms.get(cart_id) {
            return sender.subscribe();
        }
        let (sender, receiver) = broadcast::channel(CHANNEL_CAPACITY);
        rooms.insert(cart_id.to_owned(), sender);
        receiver
    }

    /// Publishes a cart event to subscribers of that cart (no-op if none).
    ///
    /// # Panics
    ///
    /// Panics if the hub mutex is poisoned.
    pub fn publish(&self, event: &CartRealtimeEvent) {
        let Ok(payload) = serde_json::to_string(event) else {
            return;
        };
        let rooms = self.rooms.lock().expect("cart hub mutex");
        if let Some(sender) = rooms.get(&event.cart.id) {
            let _ = sender.send(payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CartHub, CartRealtimeEvent};
    use crate::carts::{CartResponse, MoneyResponse};

    fn sample_cart(id: &str) -> CartResponse {
        CartResponse {
            id: id.to_owned(),
            customer_id: None,
            token: "tok".to_owned(),
            status: "open".to_owned(),
            currency: "EUR".to_owned(),
            lines: vec![],
            items_total: MoneyResponse {
                amount_minor: 0,
                currency: "EUR".to_owned(),
            },
        }
    }

    #[test]
    fn publish_reaches_subscriber() {
        let hub = CartHub::new();
        let mut rx = hub.subscribe("cart-1");
        hub.publish(&CartRealtimeEvent::updated(sample_cart("cart-1")));
        let raw = rx.try_recv().expect("event");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(parsed["type"], "cart.updated");
        assert_eq!(parsed["version"], 1);
        assert_eq!(parsed["cart"]["id"], "cart-1");
    }

    #[test]
    fn publish_without_subscriber_is_silent() {
        let hub = CartHub::new();
        hub.publish(&CartRealtimeEvent::updated(sample_cart("orphan")));
    }
}
