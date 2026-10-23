use serde::{Deserialize, Serialize};

pub type ExperimentId = String;
pub type StateHash = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditLevel {
    Minimal,
    Actions,
    ActionsAndEvidence,
    FullObservableTrace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InterventionPolicy {
    HumanControlled,
    HumanPauseOnly,
    EmergencyStopOnly,
    Autonomous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PrivacyScope {
    Public,
    Participant,
    Institution,
    Private,
    Sealed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceScope {
    pub pool_ids: Vec<String>,
    pub maximum: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityScope {
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Objective {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TerminationCondition {
    EventCount(u64),
    Explicit,
    Timestamp(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExperimentContext {
    pub experiment_id: ExperimentId,
    pub objective: Objective,
    pub participants: Vec<String>,
    pub initial_state: StateHash,
    pub resource_scope: ResourceScope,
    pub capability_scope: CapabilityScope,
    pub audit_policy: AuditLevel,
    pub intervention_policy: InterventionPolicy,
    pub privacy_scope: PrivacyScope,
    pub termination: TerminationCondition,
    pub genesis_event: String,
}

impl ExperimentContext {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        crate::hash::canonical_json_bytes(
            &serde_json::to_value(self).expect("ExperimentContext is serializable"),
        )
    }
    pub fn hash(&self) -> String {
        crate::hash::blake3_hex(&self.canonical_bytes())
    }

    pub fn validate(&self) -> Result<(), ExperimentError> {
        if self.experiment_id.trim().is_empty()
            || self.objective.description.trim().is_empty()
            || !is_digest(&self.initial_state)
            || !is_digest(&self.genesis_event)
            || self.participants.is_empty()
            || self.participants.iter().any(|id| id.trim().is_empty())
            || has_duplicates(&self.participants)
            || self
                .resource_scope
                .pool_ids
                .iter()
                .any(|id| id.trim().is_empty())
            || has_duplicates(&self.resource_scope.pool_ids)
            || self
                .capability_scope
                .capabilities
                .iter()
                .any(|id| id.trim().is_empty())
            || has_duplicates(&self.capability_scope.capabilities)
        {
            return Err(ExperimentError::InvalidPayload(
                "experiment context has missing or duplicate identity, scope, or state fields"
                    .into(),
            ));
        }
        if matches!(&self.termination, TerminationCondition::Timestamp(value) if value.trim().is_empty())
        {
            return Err(ExperimentError::InvalidPayload(
                "timestamp termination condition must be non-empty".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentPhase {
    Created,
    Frozen,
    Executed,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExperimentRecord {
    pub experiment_id: ExperimentId,
    pub context_hash: String,
    pub phase: ExperimentPhase,
    pub created_event_id: String,
    pub frozen_event_id: Option<String>,
    pub executed_event_id: Option<String>,
    pub archived_event_id: Option<String>,
    pub result_artifact_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExperimentLifecycle {
    pub experiments: std::collections::BTreeMap<ExperimentId, ExperimentRecord>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    #[error("invalid experiment event payload: {0}")]
    InvalidPayload(String),
    #[error("experiment {0} already exists")]
    Duplicate(String),
    #[error("experiment {0} does not exist")]
    Unknown(String),
    #[error("experiment {experiment_id} expected phase {expected:?}, found {actual:?}")]
    InvalidTransition {
        experiment_id: String,
        expected: ExperimentPhase,
        actual: ExperimentPhase,
    },
}

impl ExperimentLifecycle {
    pub fn apply(&mut self, event: &crate::events::Event) -> Result<(), ExperimentError> {
        match event.event_type.as_str() {
            "EXPERIMENT_CREATED" => {
                let context_value = event
                    .payload
                    .get("context")
                    .cloned()
                    .ok_or_else(|| ExperimentError::InvalidPayload("missing context".into()))?;
                let context: ExperimentContext = serde_json::from_value(context_value)
                    .map_err(|error| ExperimentError::InvalidPayload(error.to_string()))?;
                validate_context(&context)?;
                if !event.parents.contains(&context.genesis_event) {
                    return Err(ExperimentError::InvalidPayload(
                        "creation event must causally reference the declared genesis event".into(),
                    ));
                }
                let experiment_id = required_string(&event.payload, "experiment_id")?;
                let context_hash = required_string(&event.payload, "context_hash")?;
                if context.experiment_id != experiment_id || context.hash() != context_hash {
                    context.validate()?;
                    if !event.parents.contains(&context.genesis_event) {
                        return Err(ExperimentError::InvalidPayload(
                            "experiment creation must reference its genesis event as a parent"
                                .into(),
                        ));
                    }

                    return Err(ExperimentError::InvalidPayload(
                        "context identity or hash does not match the event".into(),
                    ));
                }
                if self.experiments.contains_key(&experiment_id) {
                    return Err(ExperimentError::Duplicate(experiment_id));
                }
                self.experiments.insert(
                    experiment_id.clone(),
                    ExperimentRecord {
                        experiment_id,
                        context_hash,
                        phase: ExperimentPhase::Created,
                        created_event_id: event.event_id.clone(),
                        frozen_event_id: None,
                        executed_event_id: None,
                        archived_event_id: None,
                        result_artifact_ids: Vec::new(),
                    },
                );
            }
            "EXPERIMENT_FROZEN" => {
                let record = self.require_phase(event, ExperimentPhase::Created)?;
                record.phase = ExperimentPhase::Frozen;
                record.frozen_event_id = Some(event.event_id.clone());
            }
            "EXPERIMENT_EXECUTED" => {
                let record = self.require_phase(event, ExperimentPhase::Frozen)?;
                let artifacts = event
                    .payload
                    .get("result_artifact_ids")
                    .cloned()
                    .map(serde_json::from_value::<Vec<String>>)
                    .transpose()
                    .map_err(|error| ExperimentError::InvalidPayload(error.to_string()))?
                    .unwrap_or_default();
                if artifacts.iter().any(|id| !is_digest(id)) || has_duplicates(&artifacts) {
                    return Err(ExperimentError::InvalidPayload(
                        "result artifact IDs must be unique lowercase BLAKE3 digests".into(),
                    ));
                }
                record.phase = ExperimentPhase::Executed;
                record.executed_event_id = Some(event.event_id.clone());
                record.result_artifact_ids = artifacts;
            }
            "EXPERIMENT_ARCHIVED" => {
                let record = self.require_phase(event, ExperimentPhase::Executed)?;
                record.phase = ExperimentPhase::Archived;
                record.archived_event_id = Some(event.event_id.clone());
            }
            _ => {}
        }
        Ok(())
    }

    fn require_phase(
        &mut self,
        event: &crate::events::Event,
        expected: ExperimentPhase,
    ) -> Result<&mut ExperimentRecord, ExperimentError> {
        let experiment_id = required_string(&event.payload, "experiment_id")?;
        let record = self
            .experiments
            .get_mut(&experiment_id)
            .ok_or_else(|| ExperimentError::Unknown(experiment_id.clone()))?;
        if record.context_hash != required_string(&event.payload, "context_hash")? {
            return Err(ExperimentError::InvalidPayload(
                "context hash changed during experiment lifecycle".into(),
            ));
        }
        if record.phase != expected {
            return Err(ExperimentError::InvalidTransition {
                experiment_id,
                expected,
                actual: record.phase,
            });
        }
        Ok(record)
    }

    pub fn replay(events: &[crate::events::Event]) -> Result<Self, ExperimentError> {
        let mut lifecycle = Self::default();
        for event in events {
            lifecycle.apply(event)?;
        }
        Ok(lifecycle)
    }
}

fn validate_context(context: &ExperimentContext) -> Result<(), ExperimentError> {
    if context.experiment_id.trim().is_empty()
        || context.objective.description.trim().is_empty()
        || !is_digest(&context.initial_state)
        || !is_digest(&context.genesis_event)
        || context.participants.is_empty()
        || context
            .participants
            .iter()
            .any(|value| value.trim().is_empty())
        || has_duplicates(&context.participants)
        || context
            .resource_scope
            .pool_ids
            .iter()
            .any(|value| value.trim().is_empty())
        || has_duplicates(&context.resource_scope.pool_ids)
        || context
            .capability_scope
            .capabilities
            .iter()
            .any(|value| value.trim().is_empty())
        || has_duplicates(&context.capability_scope.capabilities)
        || matches!(&context.termination, TerminationCondition::Timestamp(value) if value.trim().is_empty())
    {
        return Err(ExperimentError::InvalidPayload(
            "experiment context fields or identifiers are invalid".into(),
        ));
    }
    Ok(())
}

fn required_string(payload: &serde_json::Value, field: &str) -> Result<String, ExperimentError> {
    payload
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ExperimentError::InvalidPayload(format!("{field} must be a non-empty string"))
        })
}

fn has_duplicates(values: &[String]) -> bool {
    let unique: std::collections::BTreeSet<_> = values.iter().collect();
    unique.len() != values.len()
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::events::create_signed_event;
    use crate::identity::Identity;
    use serde_json::json;

    fn context(genesis_event: &str) -> ExperimentContext {
        ExperimentContext {
            experiment_id: "EMERGENCE-002".into(),
            objective: Objective {
                description: "Validate deterministic replay".into(),
            },
            participants: vec!["agent-a".into(), "agent-b".into()],
            initial_state: "01".repeat(32),
            resource_scope: ResourceScope {
                pool_ids: vec![],
                maximum: None,
            },
            capability_scope: CapabilityScope {
                capabilities: vec![],
            },
            audit_policy: AuditLevel::FullObservableTrace,
            intervention_policy: InterventionPolicy::EmergencyStopOnly,
            privacy_scope: PrivacyScope::Participant,
            termination: TerminationCondition::Explicit,
            genesis_event: genesis_event.into(),
        }
    }

    fn event(
        identity: &Identity,
        kind: &str,
        sequence: u64,
        clock: u64,
        parent: Option<&str>,
        payload: serde_json::Value,
    ) -> crate::events::Event {
        create_signed_event(
            identity,
            kind,
            "MOLT/0.1",
            clock,
            sequence,
            "2026-10-22T00:00:00Z",
            parent.map(|id| vec![id.to_owned()]).unwrap_or_default(),
            payload,
            vec![],
            vec![],
        )
    }

    #[test]
    fn lifecycle_replays_only_created_frozen_executed_archived_transitions() {
        let identity = Identity::generate("experiment-controller");
        let genesis = event(&identity, "GENESIS_CEREMONY", 0, 0, None, json!({}));
        let context = context(&genesis.event_id);
        let context_hash = context.hash();
        let created = event(
            &identity,
            "EXPERIMENT_CREATED",
            1,
            1,
            Some(&genesis.event_id),
            json!({
                "experiment_id": context.experiment_id,
                "context_hash": context_hash,
                "context": context,
            }),
        );
        let frozen = event(
            &identity,
            "EXPERIMENT_FROZEN",
            2,
            2,
            Some(&created.event_id),
            json!({"experiment_id":"EMERGENCE-002","context_hash":context_hash}),
        );
        let executed = event(
            &identity,
            "EXPERIMENT_EXECUTED",
            3,
            3,
            Some(&frozen.event_id),
            json!({
                "experiment_id":"EMERGENCE-002",
                "context_hash":context_hash,
                "result_artifact_ids":["ab".repeat(32)],
            }),
        );
        let archived = event(
            &identity,
            "EXPERIMENT_ARCHIVED",
            4,
            4,
            Some(&executed.event_id),
            json!({"experiment_id":"EMERGENCE-002","context_hash":context_hash}),
        );
        let lifecycle =
            ExperimentLifecycle::replay(&[genesis, created, frozen, executed, archived]).unwrap();
        let record = lifecycle.experiments.get("EMERGENCE-002").unwrap();
        assert_eq!(record.phase, ExperimentPhase::Archived);
        assert_eq!(record.result_artifact_ids, vec!["ab".repeat(32)]);
    }

    #[test]
    fn lifecycle_rejects_execution_before_freeze_and_context_mutation() {
        let identity = Identity::generate("experiment-controller");
        let genesis = event(&identity, "GENESIS_CEREMONY", 0, 0, None, json!({}));
        let context = context(&genesis.event_id);
        let created = event(
            &identity,
            "EXPERIMENT_CREATED",
            1,
            1,
            Some(&genesis.event_id),
            json!({
                "experiment_id": context.experiment_id,
                "context_hash": context.hash(),
                "context": context,
            }),
        );
        let premature = event(
            &identity,
            "EXPERIMENT_EXECUTED",
            2,
            2,
            Some(&created.event_id),
            json!({"experiment_id":"EMERGENCE-002","context_hash":context.hash()}),
        );
        let mut lifecycle = ExperimentLifecycle::default();
        lifecycle.apply(&created).unwrap();
        assert!(matches!(
            lifecycle.apply(&premature),
            Err(ExperimentError::InvalidTransition { .. })
        ));
        let tampered = event(
            &identity,
            "EXPERIMENT_FROZEN",
            2,
            2,
            Some(&created.event_id),
            json!({"experiment_id":"EMERGENCE-002","context_hash":"ff".repeat(32)}),
        );
        assert!(matches!(
            lifecycle.apply(&tampered),
            Err(ExperimentError::InvalidPayload(_))
        ));
    }
}
