//! Cart and order WebSocket hubs and typed push events.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;

use crate::carts::CartResponse;
use crate::checkout::OrderResponse;

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

/// Server-pushed order event (JSON over WebSocket).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrderRealtimeEvent {
    /// Stable event name (`order.updated`).
    #[serde(rename = "type")]
    pub event_type: String,
    /// Protocol version.
    pub version: u32,
    /// Full order snapshot after the mutation.
    pub order: OrderResponse,
}

impl OrderRealtimeEvent {
    /// Builds an `order.updated` v1 event.
    #[must_use]
    pub fn updated(order: OrderResponse) -> Self {
        Self {
            event_type: "order.updated".to_owned(),
            version: 1,
            order,
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
        subscribe_room(&self.rooms, cart_id)
    }

    /// Publishes a cart event to subscribers of that cart (no-op if none).
    ///
    /// # Panics
    ///
    /// Panics if the hub mutex is poisoned or the event fails to serialize.
    pub fn publish(&self, event: &CartRealtimeEvent) {
        let payload = serde_json::to_string(event).expect("cart event serializes");
        publish_room(&self.rooms, &event.cart.id, payload);
    }
}

/// In-process fan-out of order events by order id.
#[derive(Clone, Default)]
pub struct OrderHub {
    rooms: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
}

impl std::fmt::Debug for OrderHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rooms = self.rooms.lock().expect("order hub mutex");
        f.debug_struct("OrderHub")
            .field("room_count", &rooms.len())
            .finish()
    }
}

impl OrderHub {
    /// Creates an empty hub.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes to push events for `order_id`.
    ///
    /// # Panics
    ///
    /// Panics if the hub mutex is poisoned.
    #[must_use]
    pub fn subscribe(&self, order_id: &str) -> broadcast::Receiver<String> {
        subscribe_room(&self.rooms, order_id)
    }

    /// Publishes an order event to subscribers of that order (no-op if none).
    ///
    /// # Panics
    ///
    /// Panics if the hub mutex is poisoned or the event fails to serialize.
    pub fn publish(&self, event: &OrderRealtimeEvent) {
        let payload = serde_json::to_string(event).expect("order event serializes");
        publish_room(&self.rooms, &event.order.id, payload);
    }
}

fn subscribe_room(
    rooms: &Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
    room_id: &str,
) -> broadcast::Receiver<String> {
    let mut rooms = rooms.lock().expect("hub mutex");
    if let Some(sender) = rooms.get(room_id) {
        return sender.subscribe();
    }
    let (sender, receiver) = broadcast::channel(CHANNEL_CAPACITY);
    rooms.insert(room_id.to_owned(), sender);
    receiver
}

fn publish_room(
    rooms: &Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
    room_id: &str,
    payload: String,
) {
    let rooms = rooms.lock().expect("hub mutex");
    if let Some(sender) = rooms.get(room_id) {
        let _ = sender.send(payload);
    }
}

#[cfg(test)]
mod tests {
    use super::{CartHub, CartRealtimeEvent, OrderHub, OrderRealtimeEvent};
    use crate::carts::{CartResponse, MoneyResponse};
    use crate::checkout::OrderResponse;

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

    fn sample_order(id: &str) -> OrderResponse {
        OrderResponse {
            id: id.to_owned(),
            number: "RS-1".to_owned(),
            cart_id: None,
            state: "placed".to_owned(),
            payment_status: "pending".to_owned(),
            currency: "EUR".to_owned(),
            items_total: MoneyResponse {
                amount_minor: 100,
                currency: "EUR".to_owned(),
            },
            total: MoneyResponse {
                amount_minor: 100,
                currency: "EUR".to_owned(),
            },
            lines: vec![],
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
    fn order_publish_reaches_subscriber() {
        let hub = OrderHub::new();
        let mut rx = hub.subscribe("ord-1");
        hub.publish(&OrderRealtimeEvent::updated(sample_order("ord-1")));
        let raw = rx.try_recv().expect("event");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(parsed["type"], "order.updated");
        assert_eq!(parsed["order"]["id"], "ord-1");
        assert_eq!(parsed["order"]["total"]["amount_minor"], 100);
    }

    #[test]
    fn publish_without_subscriber_is_silent() {
        let hub = CartHub::new();
        hub.publish(&CartRealtimeEvent::updated(sample_cart("orphan")));
        let orders = OrderHub::new();
        orders.publish(&OrderRealtimeEvent::updated(sample_order("orphan")));
    }

    #[test]
    fn second_subscriber_shares_room() {
        let hub = CartHub::new();
        let mut a = hub.subscribe("cart-2");
        let mut b = hub.subscribe("cart-2");
        hub.publish(&CartRealtimeEvent::updated(sample_cart("cart-2")));
        assert!(a.try_recv().is_ok());
        assert!(b.try_recv().is_ok());
    }

    #[test]
    fn debug_lists_room_count() {
        let hub = CartHub::new();
        let _ = hub.subscribe("x");
        let text = format!("{hub:?}");
        assert!(text.contains("room_count"));
        let orders = OrderHub::new();
        let _ = orders.subscribe("y");
        assert!(format!("{orders:?}").contains("room_count"));
    }
}
