use crate::events::Event;
use crate::hash::{blake3_hex, canonical_json_bytes};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct EventQuery<'a> {
    pub actor_id: Option<&'a str>,
    pub event_type: Option<&'a str>,
    pub contains: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct EventIndex {
    events: Vec<Event>,
    by_actor: BTreeMap<String, Vec<usize>>,
    by_type: BTreeMap<String, Vec<usize>>,
    source_event_count: usize,
    source_head: Option<String>,
}

impl EventIndex {
    pub fn build(events: &[Event]) -> Self {
        let mut index = Self {
            events: events.to_vec(),
            source_event_count: events.len(),
            source_head: events.last().map(|event| event.event_id.clone()),
            ..Self::default()
        };
        for (position, event) in events.iter().enumerate() {
            index
                .by_actor
                .entry(event.actor_id.clone())
                .or_default()
                .push(position);
            index
                .by_type
                .entry(event.event_type.clone())
                .or_default()
                .push(position);
        }
        index
    }

    pub fn source_event_count(&self) -> usize {
        self.source_event_count
    }

    pub fn source_head(&self) -> Option<&str> {
        self.source_head.as_deref()
    }

    pub fn search(&self, query: EventQuery<'_>) -> Vec<&Event> {
        let candidates = match (query.actor_id, query.event_type) {
            (Some(actor), Some(kind)) => {
                let by_actor = match self.by_actor.get(actor) {
                    Some(events) => events,
                    None => return Vec::new(),
                };
                let by_type = match self.by_type.get(kind) {
                    Some(events) => events,
                    None => return Vec::new(),
                };
                if by_actor.len() <= by_type.len() {
                    by_actor
                } else {
                    by_type
                }
            }
            (Some(actor), None) => match self.by_actor.get(actor) {
                Some(events) => events,
                None => return Vec::new(),
            },
            (None, Some(kind)) => match self.by_type.get(kind) {
                Some(events) => events,
                None => return Vec::new(),
            },
            (None, None) => return self.filter_positions(0..self.events.len(), &query),
        };
        self.filter_positions(candidates.iter().copied(), &query)
    }

    pub fn verify_source(&self, events: &[Event]) -> bool {
        self.source_event_count == events.len()
            && self.source_head.as_deref() == events.last().map(|event| event.event_id.as_str())
            && self.source_hash() == source_hash(events)
    }

    fn source_hash(&self) -> String {
        source_hash(&self.events)
    }

    fn filter_positions(
        &self,
        positions: impl IntoIterator<Item = usize>,
        query: &EventQuery<'_>,
    ) -> Vec<&Event> {
        let needle = query.contains.map(str::to_lowercase);
        positions
            .into_iter()
            .filter_map(|position| self.events.get(position))
            .filter(|event| {
                query
                    .actor_id
                    .map(|actor| event.actor_id == actor)
                    .unwrap_or(true)
                    && query
                        .event_type
                        .map(|kind| event.event_type == kind)
                        .unwrap_or(true)
                    && needle
                        .as_ref()
                        .map(|needle| {
                            String::from_utf8_lossy(&canonical_json_bytes(&event.payload))
                                .to_lowercase()
                                .contains(needle)
                        })
                        .unwrap_or(true)
            })
            .collect()
    }
}

fn source_hash(events: &[Event]) -> String {
    blake3_hex(&canonical_json_bytes(
        &serde_json::to_value(events).expect("events are serializable"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::create_signed_event;
    use crate::identity::Identity;
    use serde_json::json;

    #[test]
    fn indexed_queries_match_linear_filters_and_detect_stale_source() {
        let identity = Identity::generate("agent-a");
        let first = create_signed_event(
            &identity,
            "MESSAGE_SENT",
            "MOLT/0.1",
            0,
            0,
            "2026-10-22T00:00:00Z",
            vec![],
            json!({"recipient_id":"agent-b","text":"hello"}),
            vec![],
            vec![],
        );
        let second = create_signed_event(
            &identity,
            "CLAIM_CREATED",
            "MOLT/0.1",
            1,
            1,
            "2026-10-22T00:00:01Z",
            vec![first.event_id.clone()],
            json!({"claim_id":"C-1","statement":"hello world"}),
            vec![],
            vec![],
        );
        let events = vec![first, second];
        let index = EventIndex::build(&events);
        assert!(index.verify_source(&events));
        let results = index.search(EventQuery {
            actor_id: Some("agent-a"),
            event_type: Some("CLAIM_CREATED"),
            contains: Some("WORLD"),
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type, "CLAIM_CREATED");
        assert!(!index.verify_source(&events[..1]));
    }
}
