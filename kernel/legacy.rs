use crate::hash::{blake3_hex, canonical_json_bytes};
use crate::identity::{verify, Identity, IdentityError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bequest {
    pub recipient_id: String,
    pub artifact_ids: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyRecord {
    pub schema_version: u32,
    pub agent_id: String,
    pub final_state_hash: String,
    pub selected_memory_ids: Vec<String>,
    pub contribution_artifact_ids: Vec<String>,
    pub bequests: Vec<Bequest>,
    pub lessons: Vec<String>,
    pub created_at: String,
}

impl LegacyRecord {
    pub fn validate(&self) -> Result<(), LegacyError> {
        if self.schema_version != 1 {
            return Err(LegacyError::InvalidRecord(
                "unsupported schema version".into(),
            ));
        }
        if self.agent_id.trim().is_empty()
            || !is_hash(&self.final_state_hash)
            || self.created_at.trim().is_empty()
        {
            return Err(LegacyError::InvalidRecord(
                "agent_id, a valid final_state_hash, and created_at are required".into(),
            ));
        }
        if self
            .selected_memory_ids
            .iter()
            .any(|id| id.trim().is_empty())
            || self.contribution_artifact_ids.iter().any(|id| !is_hash(id))
        {
            return Err(LegacyError::InvalidRecord(
                "memory IDs must be non-empty and contribution IDs must be content hashes".into(),
            ));
        }
        if has_duplicates(&self.selected_memory_ids)
            || has_duplicates(&self.contribution_artifact_ids)
        {
            return Err(LegacyError::InvalidRecord(
                "selected memory and contribution IDs must be unique".into(),
            ));
        }
        if self.bequests.iter().any(|bequest| {
            bequest.recipient_id.trim().is_empty()
                || bequest.artifact_ids.iter().any(|id| !is_hash(id))
                || has_duplicates(&bequest.artifact_ids)
        }) {
            return Err(LegacyError::InvalidRecord(
                "each bequest needs a recipient and unique valid artifact IDs".into(),
            ));
        }
        Ok(())
    }

    pub fn record_hash(&self) -> String {
        blake3_hex(&canonical_json_bytes(
            &serde_json::to_value(self).expect("LegacyRecord is serializable"),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedLegacyRecord {
    pub record: LegacyRecord,
    pub signer_public_key: String,
    pub signature: String,
}

impl SignedLegacyRecord {
    pub fn sign(record: LegacyRecord, identity: &Identity) -> Result<Self, LegacyError> {
        record.validate()?;
        if record.agent_id != identity.agent_id {
            return Err(LegacyError::SignerMismatch);
        }
        let bytes = canonical_json_bytes(
            &serde_json::to_value(&record).expect("LegacyRecord is serializable"),
        );
        Ok(Self {
            record,
            signer_public_key: identity.public_key_hex(),
            signature: identity.sign(&bytes),
        })
    }

    pub fn verify(&self, trusted_public_key: &str) -> Result<(), LegacyError> {
        self.record.validate()?;
        if self.signer_public_key != trusted_public_key {
            return Err(LegacyError::UntrustedSigner);
        }
        let bytes = canonical_json_bytes(
            &serde_json::to_value(&self.record).expect("LegacyRecord is serializable"),
        );
        verify(trusted_public_key, &bytes, &self.signature)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum LegacyError {
    #[error("invalid legacy record: {0}")]
    InvalidRecord(String),
    #[error("legacy record signer does not match the recorded agent")]
    SignerMismatch,
    #[error("legacy record signer is not trusted")]
    UntrustedSigner,
    #[error("legacy record signature error: {0}")]
    Identity(#[from] IdentityError),
}

fn has_duplicates(values: &[String]) -> bool {
    let unique: BTreeSet<_> = values.iter().collect();
    unique.len() != values.len()
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_record_is_signed_and_requires_an_external_trust_anchor() {
        let identity = Identity::generate("agent-17");
        let record = LegacyRecord {
            schema_version: 1,
            agent_id: identity.agent_id.clone(),
            final_state_hash: "ab".repeat(32),
            selected_memory_ids: vec!["memory-42".into()],
            contribution_artifact_ids: vec!["cd".repeat(32)],
            bequests: vec![Bequest {
                recipient_id: "agent-29".into(),
                artifact_ids: vec!["cd".repeat(32)],
                note: "Continue this work".into(),
            }],
            lessons: vec!["A passing experiment is not proof of safety".into()],
            created_at: "2026-10-22T00:00:00Z".into(),
        };
        let signed = SignedLegacyRecord::sign(record, &identity).unwrap();
        assert!(signed.verify(&identity.public_key_hex()).is_ok());
        assert!(matches!(
            signed.verify(&Identity::generate("other").public_key_hex()),
            Err(LegacyError::UntrustedSigner)
        ));
        let mut altered = signed;
        altered.record.lessons.push("Altered after signing".into());
        assert!(altered.verify(&altered.signer_public_key).is_err());
    }
}
