use crate::events::{create_signed_event, Event};
use crate::hash::{blake3_hex, canonical_json_bytes};
use crate::identity::Identity;
use crate::store::{EventStore, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

pub trait ReconcileTarget {
    type Desired: Serialize;
    type Observed: Serialize;

    fn subsystem(&self) -> &str;
    fn desired_state(&self, history: &[Event]) -> Result<Self::Desired, String>;
    fn observe(&self) -> Result<Self::Observed, String>;
    fn materialize(&mut self, desired: &Self::Desired, idempotency_key: &str)
        -> Result<(), String>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Converged,
    ApplyFailed,
    Drifted,
    ObservationFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconcileReport {
    pub subsystem: String,
    pub desired_hash: String,
    pub observed_before_hash: String,
    pub idempotency_key: String,
    pub status: Option<ReceiptStatus>,
    pub receipt_event_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum ControllerError {
    #[error("event store error: {0}")]
    Store(#[from] StoreError),
    #[error("reconciliation target failed during {stage}: {message}")]
    Target {
        stage: &'static str,
        message: String,
    },
    #[error("cannot encode reconciliation state: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("event sequence or logical clock exhausted")]
    ClockExhausted,
}

pub struct Controller;

impl Controller {
    pub fn reconcile<S, T>(
        store: &mut S,
        signer: &Identity,
        target: &mut T,
        timestamp: &str,
    ) -> Result<ReconcileReport, ControllerError>
    where
        S: EventStore,
        T: ReconcileTarget,
    {
        let history = store.all()?;
        let desired =
            target
                .desired_state(&history)
                .map_err(|message| ControllerError::Target {
                    stage: "desired-state calculation",
                    message,
                })?;
        let observed_before = target
            .observe()
            .map_err(|message| ControllerError::Target {
                stage: "observation",
                message,
            })?;
        let desired_hash = hash_value(&desired)?;
        let observed_before_hash = hash_value(&observed_before)?;
        let subsystem = target.subsystem().to_owned();
        let idempotency_key = blake3_hex(&canonical_json_bytes(&json!({
            "desired_hash": desired_hash,
            "subsystem": subsystem,
        })));

        if desired_hash == observed_before_hash {
            return Ok(ReconcileReport {
                subsystem,
                desired_hash,
                observed_before_hash,
                idempotency_key,
                status: None,
                receipt_event_id: None,
            });
        }

        let apply_succeeded = target.materialize(&desired, &idempotency_key).is_ok();
        let observed_after = target.observe().ok();
        let observed_after_hash = observed_after.as_ref().map(hash_value).transpose()?;
        let status = match (&observed_after, &observed_after_hash) {
            (Some(_), Some(hash)) if hash == &desired_hash => ReceiptStatus::Converged,
            (Some(_), _) if !apply_succeeded => ReceiptStatus::ApplyFailed,
            (Some(_), _) => ReceiptStatus::Drifted,
            (None, _) => ReceiptStatus::ObservationFailed,
        };

        let last = history.iter().max_by_key(|event| event.sequence_number);
        let sequence_number = last
            .map(|event| event.sequence_number.checked_add(1))
            .unwrap_or(Some(0))
            .ok_or(ControllerError::ClockExhausted)?;
        let logical_clock = last
            .map(|event| event.logical_clock.checked_add(1))
            .unwrap_or(Some(0))
            .ok_or(ControllerError::ClockExhausted)?;
        let parents = last
            .map(|event| vec![event.event_id.clone()])
            .unwrap_or_default();
        let receipt = create_signed_event(
            signer,
            "MOLT_RECONCILIATION_RECEIPT",
            "MOLT/0.1",
            logical_clock,
            sequence_number,
            timestamp,
            parents,
            json!({
                "subsystem": subsystem,
                "desired_hash": desired_hash,
                "observed_before_hash": observed_before_hash,
                "observed_after_hash": observed_after_hash,
                "idempotency_key": idempotency_key,
                "apply_succeeded": apply_succeeded,
                "status": status,
            }),
            vec![],
            vec![],
        );
        store.append(receipt.clone())?;

        Ok(ReconcileReport {
            subsystem,
            desired_hash,
            observed_before_hash,
            idempotency_key,
            status: Some(status),
            receipt_event_id: Some(receipt.event_id),
        })
    }
}

fn hash_value<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    Ok(blake3_hex(&canonical_json_bytes(&serde_json::to_value(
        value,
    )?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;
    use crate::store::{EventStore, MemoryEventStore};

    #[derive(Default)]
    struct Target {
        actual: Vec<String>,
        fail: bool,
    }

    impl ReconcileTarget for Target {
        type Desired = Vec<String>;
        type Observed = Vec<String>;

        fn subsystem(&self) -> &str {
            "test-subsystem"
        }

        fn desired_state(&self, _history: &[Event]) -> Result<Self::Desired, String> {
            Ok(vec!["resource-a".into()])
        }

        fn observe(&self) -> Result<Self::Observed, String> {
            Ok(self.actual.clone())
        }

        fn materialize(&mut self, desired: &Self::Desired, _key: &str) -> Result<(), String> {
            if self.fail {
                return Err("deliberately withheld".into());
            }
            self.actual = desired.clone();
            Ok(())
        }
    }

    #[test]
    fn materialization_is_reconciled_and_receipted_once() {
        let identity = Identity::generate("controller");
        let mut store = MemoryEventStore::new();
        let mut target = Target::default();
        let first =
            Controller::reconcile(&mut store, &identity, &mut target, "2026-10-22T00:00:00Z")
                .unwrap();
        assert_eq!(first.status, Some(ReceiptStatus::Converged));
        assert!(first.receipt_event_id.is_some());
        let event = store
            .get(&first.receipt_event_id.clone().unwrap())
            .unwrap()
            .unwrap();
        event.verify(&identity.public_key_hex()).unwrap();
        let second =
            Controller::reconcile(&mut store, &identity, &mut target, "2026-10-22T00:00:01Z")
                .unwrap();
        assert_eq!(second.status, None);
        assert_eq!(second.receipt_event_id, None);
        assert_eq!(store.all().unwrap().len(), 1);
        assert_eq!(first.idempotency_key, second.idempotency_key);
    }

    #[test]
    fn failed_materialization_leaves_a_signed_failure_receipt() {
        let identity = Identity::generate("controller");
        let mut store = MemoryEventStore::new();
        let mut target = Target {
            fail: true,
            ..Target::default()
        };
        let report =
            Controller::reconcile(&mut store, &identity, &mut target, "2026-10-22T00:00:00Z")
                .unwrap();
        assert_eq!(report.status, Some(ReceiptStatus::ApplyFailed));
        let events = store.all().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["status"], "apply_failed");
        events[0].verify(&identity.public_key_hex()).unwrap();
    }
}
