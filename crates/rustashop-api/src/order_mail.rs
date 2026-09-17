//! Order confirmation mail via `serenade-mailer`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serenade_mailer::{Email, FileTransport, MailerError, NullTransport, Transport};

use crate::checkout::OrderResponse;

/// Env: From address for order mail (default `orders@rustashop.local`).
pub const ORDER_MAIL_FROM_ENV: &str = "RUSTASHOP_ORDER_MAIL_FROM";
/// Env: when set, dump messages under this directory (`FileTransport`); else null.
pub const ORDER_MAIL_DIR_ENV: &str = "RUSTASHOP_MAIL_DIR";

const DEFAULT_FROM: &str = "orders@rustashop.local";

/// Sends checkout confirmation emails (null / file / recording).
#[derive(Clone)]
pub struct OrderMailer {
    transport: Arc<dyn Transport>,
    from: String,
}

impl std::fmt::Debug for OrderMailer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OrderMailer")
            .field("from", &self.from)
            .finish_non_exhaustive()
    }
}

impl OrderMailer {
    /// Null transport for local/CI (discards after validating From/To).
    #[must_use]
    pub fn null() -> Self {
        Self::with_transport(Arc::new(NullTransport::new()), DEFAULT_FROM)
    }

    /// From env: `FileTransport` when [`ORDER_MAIL_DIR_ENV`] is set, else null.
    #[must_use]
    pub fn from_env() -> Self {
        let from = std::env::var(ORDER_MAIL_FROM_ENV).unwrap_or_else(|_| DEFAULT_FROM.to_owned());
        match std::env::var(ORDER_MAIL_DIR_ENV) {
            Ok(dir) if !dir.trim().is_empty() => {
                Self::with_transport(Arc::new(FileTransport::new(PathBuf::from(dir))), from)
            }
            _ => Self::with_transport(Arc::new(NullTransport::new()), from),
        }
    }

    /// Explicit transport (tests / DI).
    #[must_use]
    pub fn with_transport(transport: Arc<dyn Transport>, from: impl Into<String>) -> Self {
        Self {
            transport,
            from: from.into(),
        }
    }

    /// Recording transport for unit tests.
    #[must_use]
    pub fn recording(outbox: Arc<Mutex<Vec<Email>>>) -> Self {
        Self::with_transport(Arc::new(RecordingTransport { outbox }), DEFAULT_FROM)
    }

    /// Sends a plain-text order confirmation to `to`.
    ///
    /// # Errors
    ///
    /// Propagates transport / address validation failures.
    pub fn send_order_confirmation(
        &self,
        order: &OrderResponse,
        to: &str,
    ) -> Result<(), MailerError> {
        let subject = format!("Order {} confirmed", order.number);
        let text = format!(
            "Thank you for your order.\n\nOrder: {}\nId: {}\nTotal: {} {}\n",
            order.number, order.id, order.total.amount_minor, order.total.currency
        );
        let email = Email::new()
            .from(self.from.as_str())?
            .to(to)?
            .subject(subject)
            .text(text);
        self.transport.send(&email)
    }
}

#[derive(Clone)]
struct RecordingTransport {
    outbox: Arc<Mutex<Vec<Email>>>,
}

impl Transport for RecordingTransport {
    fn send(&self, email: &Email) -> Result<(), MailerError> {
        if email.from_addresses().is_empty() {
            return Err(MailerError::MissingSender);
        }
        if email.to_addresses().is_empty() {
            return Err(MailerError::MissingRecipient);
        }
        self.outbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(email.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carts::MoneyResponse;

    fn sample_order() -> OrderResponse {
        OrderResponse {
            id: "oid".into(),
            number: "RS-1".into(),
            cart_id: None,
            state: "placed".into(),
            payment_status: "pending".into(),
            currency: "EUR".into(),
            items_total: MoneyResponse {
                amount_minor: 100,
                currency: "EUR".into(),
            },
            total: MoneyResponse {
                amount_minor: 100,
                currency: "EUR".into(),
            },
            lines: Vec::new(),
        }
    }

    #[test]
    fn null_and_recording_send() {
        let mailer = OrderMailer::null();
        assert!(format!("{mailer:?}").contains("OrderMailer"));
        mailer
            .send_order_confirmation(&sample_order(), "buyer@example.test")
            .expect("null");

        let outbox = Arc::new(Mutex::new(Vec::new()));
        let recording = OrderMailer::recording(Arc::clone(&outbox));
        recording
            .send_order_confirmation(&sample_order(), "buyer@example.test")
            .expect("record");
        let (len, subject) = {
            let sent = outbox.lock().expect("lock");
            (sent.len(), sent[0].subject_line().to_owned())
        };
        assert_eq!(len, 1);
        assert!(subject.contains("RS-1"));
    }

    #[test]
    fn from_env_builds() {
        let _ = OrderMailer::from_env();
    }
}
