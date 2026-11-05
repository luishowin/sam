//! Structured execution results (§0.0.6-A).
//!
//! Every executor returns an `ExecutionResult`. The UI never has to guess
//! whether something happened: a failed file operation is `Failed` with a
//! cause, not an exception, not silence, and never a "Done!" for work that
//! did not happen.

use std::collections::BTreeMap;
use std::fmt;

use crate::entity::Entity;

/// Truthful execution state. `Success` means the effect was verified where a
/// check exists (see per-capability `verify`); otherwise it means the
/// executor carried the approved action out without error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Success,
    Failed,
    Blocked,
    Cancelled,
    Unknown,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Status::Success => "success",
            Status::Failed => "failed",
            Status::Blocked => "blocked",
            Status::Cancelled => "cancelled",
            Status::Unknown => "unknown",
        };
        f.write_str(s)
    }
}

/// What running an approved action produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionResult {
    pub status: Status,
    /// Entities the execution produced or affected (opened file, launched
    /// app). Empty when there is nothing to refer back to.
    pub entities: Vec<Entity>,
    /// Human-readable outcome for the panel ("Locked", "22 °C and …").
    /// `None` when the row itself already says what happened.
    pub output: Option<String>,
    /// Why it did not succeed. Present exactly when `status != Success`.
    pub error: Option<String>,
    /// Machine-readable extras (timing, scope name). Never arguments (F6).
    pub metadata: BTreeMap<String, String>,
}

impl ExecutionResult {
    pub fn success() -> Self {
        ExecutionResult {
            status: Status::Success,
            entities: Vec::new(),
            output: None,
            error: None,
            metadata: BTreeMap::new(),
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        ExecutionResult {
            status: Status::Failed,
            entities: Vec::new(),
            output: None,
            error: Some(error.into()),
            ..Self::success()
        }
    }

    pub fn blocked(error: impl Into<String>) -> Self {
        ExecutionResult {
            status: Status::Blocked,
            ..Self::failed(error)
        }
    }

    pub fn cancelled() -> Self {
        ExecutionResult {
            status: Status::Cancelled,
            ..Self::success()
        }
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    pub fn with_entity(mut self, entity: Entity) -> Self {
        self.entities.push(entity);
        self
    }

    pub fn with_meta(mut self, key: &str, value: impl Into<String>) -> Self {
        self.metadata.insert(key.to_owned(), value.into());
        self
    }

    pub fn succeeded(&self) -> bool {
        self.status == Status::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_names_a_cause() {
        let r = ExecutionResult::failed("destination folder does not exist");
        assert!(!r.succeeded());
        assert_eq!(r.error.as_deref(), Some("destination folder does not exist"));
        assert_eq!(r.status.to_string(), "failed");
    }

    #[test]
    fn success_carries_entities_for_later_reference() {
        let e = Entity::new("uri:file:///a.pdf", crate::entity::EntityType::File, "a.pdf");
        let r = ExecutionResult::success()
            .with_output("Opened")
            .with_entity(e.clone());
        assert!(r.succeeded());
        assert_eq!(r.entities, vec![e]);
    }

    #[test]
    fn blocked_and_cancelled_are_not_success() {
        assert!(!ExecutionResult::blocked("AI mode is off").succeeded());
        assert!(!ExecutionResult::cancelled().succeeded());
    }
}
