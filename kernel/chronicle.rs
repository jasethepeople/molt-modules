use crate::events::Event;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChronicleEntry {
    pub event_id: String,
    pub sequence_number: u64,
    pub timestamp: String,
    pub actor_id: String,
    pub event_type: String,
    pub narrative: String,
}

pub fn from_events(events: &[Event]) -> Vec<ChronicleEntry> {
    events
        .iter()
        .map(|event| ChronicleEntry {
            event_id: event.event_id.clone(),
            sequence_number: event.sequence_number,
            timestamp: event.timestamp.clone(),
            actor_id: event.actor_id.clone(),
            event_type: event.event_type.clone(),
            narrative: describe(event),
        })
        .collect()
}

fn describe(event: &Event) -> String {
    let payload = &event.payload;
    match event.event_type.as_str() {
        "RESOURCE_POOL_CREATED" => format!(
            "{} created resource pool {} with capacity {} {}.",
            event.actor_id,
            text(payload, "pool_id"),
            text(payload, "capacity"),
            text(payload, "resource_type")
        ),
        "RESOURCE_LEASE_ISSUED" => format!(
            "{} issued lease {} to {} for {} units from pool {}.",
            event.actor_id,
            text(payload, "lease_id"),
            text(payload, "grantee_id"),
            text(payload, "amount"),
            text(payload, "pool_id")
        ),
        "RESOURCE_CONSUMED" => format!(
            "{} consumed {} units from lease {}.",
            event.actor_id,
            text(payload, "amount"),
            text(payload, "lease_id")
        ),
        "MESSAGE_SENT" => format!(
            "{} sent a message to {}.",
            event.actor_id,
            text(payload, "recipient_id")
        ),
        "ARTIFACT_MANIFEST_CREATED" => format!(
            "{} recorded artifact {} with manifest {}.",
            event.actor_id,
            text(payload, "artifact_id"),
            text(payload, "manifest_id")
        ),
        "MOLT_RECONCILIATION_RECEIPT" => format!(
            "{} recorded {} reconciliation for {}.",
            event.actor_id,
            text(payload, "status"),
            text(payload, "subsystem")
        ),
        "EXPERIMENT_CREATED" => format!(
            "{} created experiment {}.",
            event.actor_id,
            text(payload, "experiment_id")
        ),
        "EXPERIMENT_FROZEN" => format!(
            "{} froze experiment {}.",
            event.actor_id,
            text(payload, "experiment_id")
        ),
        "EXPERIMENT_EXECUTED" => format!(
            "{} recorded execution of experiment {}.",
            event.actor_id,
            text(payload, "experiment_id")
        ),
        "EXPERIMENT_ARCHIVED" => format!(
            "{} archived experiment {}.",
            event.actor_id,
            text(payload, "experiment_id")
        ),
        _ => format!(
            "{} emitted {} (event {}).",
            event.actor_id, event.event_type, event.event_id
        ),
    }
}

fn text(payload: &Value, field: &str) -> String {
    match payload.get(field) {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(_) => "[structured value]".into(),
        None => "[missing]".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::create_signed_event;
    use crate::identity::Identity;
    use serde_json::json;

    #[test]
    fn chronicle_keeps_event_provenance_and_never_invents_unknown_details() {
        let identity = Identity::generate("agent-a");
        let event = create_signed_event(
            &identity,
            "RESOURCE_LEASE_ISSUED",
            "MOLT/0.1",
            0,
            0,
            "2026-10-22T00:00:00Z",
            vec![],
            json!({"lease_id":"L1","grantee_id":"agent-b","amount":5,"pool_id":"P1"}),
            vec![],
            vec![],
        );
        let entries = from_events(std::slice::from_ref(&event));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event_id, event.event_id);
        assert!(entries[0].narrative.contains("agent-a issued lease L1"));

        let unknown = Event {
            event_type: "FUTURE_EVENT".into(),
            ..event
        };
        assert!(describe(&unknown).contains("emitted FUTURE_EVENT"));
        assert!(describe(&unknown).contains(&unknown.event_id));
    }
}
