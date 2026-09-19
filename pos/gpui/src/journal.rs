//! Append-only ticket journal with a hash-chain stub (not NF525-certified).

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

/// One line on a persisted ticket.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketLine {
    /// Variant id.
    pub variant_id: String,
    /// SKU.
    pub sku: String,
    /// Product name.
    pub product_name: String,
    /// Quantity.
    pub quantity: i32,
    /// Unit price minor units.
    pub unit_price_minor: i64,
    /// Line total minor units.
    pub line_total_minor: i64,
    /// ISO currency.
    pub currency: String,
}

/// Ticket written to the journal.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketRecord {
    /// Ticket id (UUID-like string).
    pub id: String,
    /// Sequence number in the journal.
    pub seq: u64,
    /// RFC3339 timestamp.
    pub created_at: String,
    /// Lines.
    pub lines: Vec<TicketLine>,
    /// Payable total minor units.
    pub total_minor: i64,
    /// Currency.
    pub currency: String,
    /// Amount tendered (minor units).
    pub tendered_minor: i64,
    /// Change given (minor units).
    pub change_minor: i64,
    /// Hex SHA-256 of previous record (`GENESIS` for the first).
    pub prev_hash: String,
    /// Hex SHA-256 of this record payload (excluding `hash` itself).
    pub hash: String,
}

/// Period closure summary (fiscal close stub).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClosureSummary {
    /// Closure id.
    pub id: String,
    /// Sequence number.
    pub seq: u64,
    /// RFC3339 timestamp.
    pub created_at: String,
    /// Tickets included since previous closure (or genesis).
    pub ticket_count: u64,
    /// Sum of ticket totals (minor units).
    pub total_minor: i64,
    /// Currency of the totals (first ticket currency or EUR).
    pub currency: String,
    /// Previous record hash.
    pub prev_hash: String,
    /// This record hash.
    pub hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JournalEntry {
    Ticket(TicketRecord),
    Closure(ClosureSummary),
}

/// Journal I/O errors.
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    /// Filesystem failure.
    #[error("journal io: {0}")]
    Io(#[from] std::io::Error),
    /// Corrupt or undecodable line.
    #[error("journal parse: {0}")]
    Parse(String),
    /// Empty sale cannot be ticketed.
    #[error("sale is empty")]
    EmptySale,
}

/// Append-only JSONL journal on disk.
pub struct Journal {
    path: PathBuf,
    entries: Vec<JournalEntry>,
}

impl Journal {
    /// Opens or creates a journal file and loads existing entries.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read or a line is corrupt.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, JournalError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut entries = Vec::new();
        if path.exists() {
            let file = File::open(&path)?;
            for (idx, line) in BufReader::new(file).lines().enumerate() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let entry: JournalEntry = serde_json::from_str(&line)
                    .map_err(|e| JournalError::Parse(format!("line {}: {e}", idx + 1)))?;
                entries.push(entry);
            }
        }
        Ok(Self { path, entries })
    }

    /// Journal file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of stored entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the journal is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Tickets not yet covered by a later closure.
    #[must_use]
    pub fn open_tickets(&self) -> Vec<&TicketRecord> {
        let last_closure_seq = self
            .entries
            .iter()
            .rev()
            .find_map(|e| match e {
                JournalEntry::Closure(c) => Some(c.seq),
                JournalEntry::Ticket(_) => None,
            })
            .unwrap_or(0);
        self.entries
            .iter()
            .filter_map(|e| match e {
                JournalEntry::Ticket(t) if t.seq > last_closure_seq => Some(t),
                _ => None,
            })
            .collect()
    }

    /// Recent tickets (newest first, capped).
    #[must_use]
    pub fn recent_tickets(&self, limit: usize) -> Vec<&TicketRecord> {
        self.entries
            .iter()
            .rev()
            .filter_map(|e| match e {
                JournalEntry::Ticket(t) => Some(t),
                JournalEntry::Closure(_) => None,
            })
            .take(limit)
            .collect()
    }

    fn last_hash(&self) -> String {
        match self.entries.last() {
            Some(JournalEntry::Ticket(t)) => t.hash.clone(),
            Some(JournalEntry::Closure(c)) => c.hash.clone(),
            None => "GENESIS".into(),
        }
    }

    const fn next_seq(&self) -> u64 {
        self.entries.len() as u64 + 1
    }

    /// Appends a ticket for the given lines and tender (exact or overpay).
    ///
    /// # Errors
    ///
    /// Returns [`JournalError::EmptySale`] when `lines` is empty, or an I/O error on append.
    pub fn append_ticket(
        &mut self,
        lines: Vec<TicketLine>,
        tendered_minor: i64,
    ) -> Result<TicketRecord, JournalError> {
        if lines.is_empty() {
            return Err(JournalError::EmptySale);
        }
        let total_minor: i64 = lines.iter().map(|l| l.line_total_minor).sum();
        let currency = lines
            .first()
            .map_or_else(|| "EUR".into(), |l| l.currency.clone());
        let change_minor = tendered_minor.saturating_sub(total_minor);
        let seq = self.next_seq();
        let prev_hash = self.last_hash();
        let created_at = OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown".into());
        let id = format!("T-{seq:06}");
        let mut ticket = TicketRecord {
            id,
            seq,
            created_at,
            lines,
            total_minor,
            currency,
            tendered_minor,
            change_minor,
            prev_hash,
            hash: String::new(),
        };
        ticket.hash = hash_ticket(&ticket);
        self.append_entry(JournalEntry::Ticket(ticket.clone()))?;
        Ok(ticket)
    }

    /// Writes a period close covering tickets since the previous closure.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the append fails.
    pub fn append_closure(&mut self) -> Result<ClosureSummary, JournalError> {
        let open = self.open_tickets();
        let ticket_count = open.len() as u64;
        let total_minor: i64 = open.iter().map(|t| t.total_minor).sum();
        let currency = open
            .first()
            .map_or_else(|| "EUR".into(), |t| t.currency.clone());
        let seq = self.next_seq();
        let prev_hash = self.last_hash();
        let created_at = OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown".into());
        let id = format!("C-{seq:06}");
        let mut closure = ClosureSummary {
            id,
            seq,
            created_at,
            ticket_count,
            total_minor,
            currency,
            prev_hash,
            hash: String::new(),
        };
        closure.hash = hash_closure(&closure);
        self.append_entry(JournalEntry::Closure(closure.clone()))?;
        Ok(closure)
    }

    fn append_entry(&mut self, entry: JournalEntry) -> Result<(), JournalError> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(&entry)
            .map_err(|e| JournalError::Parse(e.to_string()))?;
        writeln!(file, "{line}")?;
        self.entries.push(entry);
        Ok(())
    }
}

fn hash_ticket(ticket: &TicketRecord) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ticket.id.as_bytes());
    hasher.update(ticket.seq.to_le_bytes());
    hasher.update(ticket.created_at.as_bytes());
    hasher.update(ticket.total_minor.to_le_bytes());
    hasher.update(ticket.tendered_minor.to_le_bytes());
    hasher.update(ticket.prev_hash.as_bytes());
    for line in &ticket.lines {
        hasher.update(line.variant_id.as_bytes());
        hasher.update(line.quantity.to_le_bytes());
        hasher.update(line.line_total_minor.to_le_bytes());
    }
    hex::encode(hasher.finalize())
}

fn hash_closure(closure: &ClosureSummary) -> String {
    let mut hasher = Sha256::new();
    hasher.update(closure.id.as_bytes());
    hasher.update(closure.seq.to_le_bytes());
    hasher.update(closure.created_at.as_bytes());
    hasher.update(closure.ticket_count.to_le_bytes());
    hasher.update(closure.total_minor.to_le_bytes());
    hasher.update(closure.prev_hash.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    fn sample_line() -> TicketLine {
        TicketLine {
            variant_id: "v1".into(),
            sku: "SKU".into(),
            product_name: "Item".into(),
            quantity: 1,
            unit_price_minor: 500,
            line_total_minor: 500,
            currency: "EUR".into(),
        }
    }

    #[test]
    fn ticket_and_closure_chain() {
        let path = temp_dir().join(format!("rustashop-pos-journal-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut journal = Journal::open(&path).expect("open");
        let t1 = journal
            .append_ticket(vec![sample_line()], 500)
            .expect("ticket");
        assert_eq!(t1.prev_hash, "GENESIS");
        assert_ne!(t1.hash, "");
        let t2 = journal
            .append_ticket(vec![sample_line()], 1000)
            .expect("ticket2");
        assert_eq!(t2.prev_hash, t1.hash);
        assert_eq!(journal.open_tickets().len(), 2);
        let closure = journal.append_closure().expect("closure");
        assert_eq!(closure.ticket_count, 2);
        assert_eq!(closure.total_minor, 1000);
        assert_eq!(journal.open_tickets().len(), 0);
        let reopened = Journal::open(&path).expect("reopen");
        assert_eq!(reopened.len(), 3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_sale_rejected() {
        let path = temp_dir().join(format!("rustashop-pos-empty-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut journal = Journal::open(&path).expect("open");
        assert!(matches!(
            journal.append_ticket(Vec::new(), 0),
            Err(JournalError::EmptySale)
        ));
        let _ = std::fs::remove_file(&path);
    }
}
