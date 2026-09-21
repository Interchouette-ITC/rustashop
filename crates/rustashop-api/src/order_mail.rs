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
        Self::from_parts(from, std::env::var(ORDER_MAIL_DIR_ENV).ok())
    }

    /// Builds from an explicit From address and optional dump directory.
    #[must_use]
    pub fn from_parts(from: impl Into<String>, mail_dir: Option<String>) -> Self {
        let from = from.into();
        match mail_dir {
            Some(dir) if !dir.trim().is_empty() => {
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

    /// Transport that always fails (tests the checkout warn path).
    #[must_use]
    pub fn failing() -> Self {
        Self::with_transport(Arc::new(FailingTransport), DEFAULT_FROM)
    }

    /// Sends a multipart (text + HTML) order confirmation to `to`.
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
        let html = format!(
            "<p>Thank you for your order.</p>\
             <p><strong>Order:</strong> {number}<br>\
             <strong>Id:</strong> {id}<br>\
             <strong>Total:</strong> {amount} {currency}</p>",
            number = order.number,
            id = order.id,
            amount = order.total.amount_minor,
            currency = order.total.currency,
        );
        let email = Email::new()
            .from(self.from.as_str())?
            .to(to)?
            .subject(subject)
            .text(text)
            .html(html);
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

#[derive(Clone, Copy, Debug)]
struct FailingTransport;

impl Transport for FailingTransport {
    fn send(&self, _email: &Email) -> Result<(), MailerError> {
        Err(MailerError::MissingSender)
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
        let mime = {
            let sent = outbox.lock().expect("lock");
            sent[0].mime_tree().multipart_subtype()
        };
        assert_eq!(mime, Some("alternative"));
        let parts = {
            let sent = outbox.lock().expect("lock");
            (
                sent[0].text_part().map(str::to_owned),
                sent[0].html_part().map(str::to_owned),
            )
        };
        assert!(parts.0.is_some_and(|text| text.contains("RS-1")));
        assert!(
            parts
                .1
                .is_some_and(|html| html.contains("<strong>Order:</strong>"))
        );
    }

    #[test]
    fn recording_requires_from_and_to() {
        let outbox = Arc::new(Mutex::new(Vec::new()));
        let transport = RecordingTransport {
            outbox: Arc::clone(&outbox),
        };
        assert!(matches!(
            transport.send(&Email::new()),
            Err(MailerError::MissingSender)
        ));
        let from_only = Email::new().from("from@example.test").expect("from");
        assert!(matches!(
            transport.send(&from_only),
            Err(MailerError::MissingRecipient)
        ));
    }

    #[test]
    fn from_parts_file_and_null() {
        let dir = std::env::temp_dir().join(format!(
            "rustashop-mail-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let mailer = OrderMailer::from_parts(DEFAULT_FROM, Some(dir.to_string_lossy().into()));
        mailer
            .send_order_confirmation(&sample_order(), "buyer@example.test")
            .expect("file transport");
        assert!(dir.exists());
        let _ = std::fs::remove_dir_all(&dir);

        let nullish = OrderMailer::from_parts(DEFAULT_FROM, Some("   ".into()));
        nullish
            .send_order_confirmation(&sample_order(), "buyer@example.test")
            .expect("blank dir is null");
        let _ = OrderMailer::from_env();
    }

    #[test]
    fn failing_transport_errors() {
        assert!(
            OrderMailer::failing()
                .send_order_confirmation(&sample_order(), "buyer@example.test")
                .is_err()
        );
    }
}
