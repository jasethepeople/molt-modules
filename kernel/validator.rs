use crate::hash::{blake3_hex, canonical_json_bytes};
use crate::identity::{verify, Identity, IdentityError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceItem {
    pub evidence_id: String,
    pub content: Value,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidationRule {
    ClaimFieldEquals {
        field: String,
        expected: Value,
    },
    ClaimHashEquals {
        expected_hash: String,
    },
    EvidenceExists {
        evidence_id: String,
    },
    EvidenceFieldEquals {
        evidence_id: String,
        field: String,
        expected: Value,
    },
    EvidenceHashEquals {
        evidence_id: String,
        expected_hash: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationCase {
    pub case_id: String,
    pub claim: Value,
    pub evidence: Vec<EvidenceItem>,
    pub rules: BTreeMap<String, ValidationRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleResult {
    pub passed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationVerdict {
    pub version: u32,
    pub case_id: String,
    pub case_hash: String,
    pub validator_id: String,
    pub validator_public_key: String,
    pub passed: bool,
    pub rules: BTreeMap<String, RuleResult>,
    pub signature: String,
}

#[derive(Debug, Error)]
pub enum ValidatorError {
    #[error("invalid validation case: {0}")]
    InvalidCase(String),
    #[error("verdict signature is invalid: {0}")]
    Signature(#[from] IdentityError),
    #[error("verdict was computed for a different case")]
    CaseMismatch,
}

impl ValidationCase {
    pub fn hash(&self) -> Result<String, ValidatorError> {
        validate_case(self)?;
        Ok(blake3_hex(&canonical_json_bytes(
            &serde_json::to_value(self).expect("serializable validation case"),
        )))
    }
}

pub fn validate_and_sign(
    case: &ValidationCase,
    validator: &Identity,
) -> Result<ValidationVerdict, ValidatorError> {
    validate_case(case)?;
    let mut rule_results = BTreeMap::new();
    for (rule_id, rule) in &case.rules {
        let result = evaluate(rule, &case.claim, &case.evidence);
        rule_results.insert(rule_id.clone(), result);
    }
    let case_hash = blake3_hex(&canonical_json_bytes(
        &serde_json::to_value(case).expect("serializable validation case"),
    ));
    let mut verdict = ValidationVerdict {
        version: 1,
        case_id: case.case_id.clone(),
        case_hash,
        validator_id: validator.agent_id.clone(),
        validator_public_key: validator.public_key_hex(),
        passed: rule_results.values().all(|result| result.passed),
        rules: rule_results,
        signature: String::new(),
    };
    verdict.signature = validator.sign(&verdict_signing_bytes(&verdict));
    Ok(verdict)
}

pub fn verify_verdict(
    verdict: &ValidationVerdict,
    case: &ValidationCase,
    trusted_public_key: &str,
) -> Result<(), ValidatorError> {
    if verdict.version != 1
        || verdict.validator_public_key != trusted_public_key
        || verdict.validator_id.trim().is_empty()
        || verdict.case_id != case.case_id
        || verdict.case_hash != case.hash()?
        || verdict.passed != verdict.rules.values().all(|result| result.passed)
    {
        return Err(ValidatorError::CaseMismatch);
    }
    verify(
        trusted_public_key,
        &verdict_signing_bytes(verdict),
        &verdict.signature,
    )?;
    Ok(())
}

fn verdict_signing_bytes(verdict: &ValidationVerdict) -> Vec<u8> {
    let mut unsigned = verdict.clone();
    unsigned.signature.clear();
    canonical_json_bytes(&serde_json::to_value(unsigned).expect("serializable verdict"))
}

fn validate_case(case: &ValidationCase) -> Result<(), ValidatorError> {
    if case.case_id.trim().is_empty() {
        return Err(ValidatorError::InvalidCase(
            "case ID must be non-empty".into(),
        ));
    }
    if case.claim.is_null() {
        return Err(ValidatorError::InvalidCase("claim must not be null".into()));
    }
    if case.rules.is_empty() {
        return Err(ValidatorError::InvalidCase(
            "at least one deterministic validation rule is required".into(),
        ));
    }
    let mut evidence = BTreeMap::new();
    for item in &case.evidence {
        if item.evidence_id.trim().is_empty() {
            return Err(ValidatorError::InvalidCase(
                "evidence IDs must be non-empty".into(),
            ));
        }
        let actual_hash = blake3_hex(&canonical_json_bytes(&item.content));
        if item.content_hash != actual_hash {
            return Err(ValidatorError::InvalidCase(format!(
                "evidence {} content hash does not match its content",
                item.evidence_id
            )));
        }
        if evidence.insert(&item.evidence_id, item).is_some() {
            return Err(ValidatorError::InvalidCase(format!(
                "duplicate evidence ID {}",
                item.evidence_id
            )));
        }
    }
    if case.rules.keys().any(|rule_id| rule_id.trim().is_empty()) {
        return Err(ValidatorError::InvalidCase(
            "rule IDs must be non-empty".into(),
        ));
    }
    for rule in case.rules.values() {
        match rule {
            ValidationRule::ClaimFieldEquals { field, .. } => {
                if field.trim().is_empty() {
                    return Err(ValidatorError::InvalidCase(
                        "claim field rules need a field name".into(),
                    ));
                }
            }
            ValidationRule::ClaimHashEquals { expected_hash } => {
                if !is_digest(expected_hash) {
                    return Err(ValidatorError::InvalidCase(
                        "claim hash rules need a lowercase BLAKE3 digest".into(),
                    ));
                }
            }
            ValidationRule::EvidenceExists { evidence_id } => {
                if evidence_id.trim().is_empty() {
                    return Err(ValidatorError::InvalidCase(
                        "evidence rule IDs must be non-empty".into(),
                    ));
                }
            }
            ValidationRule::EvidenceFieldEquals {
                evidence_id, field, ..
            } => {
                if evidence_id.trim().is_empty() || field.trim().is_empty() {
                    return Err(ValidatorError::InvalidCase(
                        "field equality rules need an evidence ID and field".into(),
                    ));
                }
            }
            ValidationRule::EvidenceHashEquals {
                evidence_id,
                expected_hash,
            } => {
                if evidence_id.trim().is_empty() || !is_digest(expected_hash) {
                    return Err(ValidatorError::InvalidCase(
                        "hash rules need an evidence ID and a 64-character hexadecimal digest"
                            .into(),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn evaluate(rule: &ValidationRule, claim: &Value, evidence: &[EvidenceItem]) -> RuleResult {
    let item = match rule {
        ValidationRule::ClaimFieldEquals { .. } | ValidationRule::ClaimHashEquals { .. } => None,
        ValidationRule::EvidenceExists { evidence_id }
        | ValidationRule::EvidenceFieldEquals { evidence_id, .. }
        | ValidationRule::EvidenceHashEquals { evidence_id, .. } => evidence
            .iter()
            .find(|item| &item.evidence_id == evidence_id),
    };
    let (passed, reason) = match rule {
        ValidationRule::ClaimFieldEquals { field, expected } => match claim.get(field) {
            Some(actual) if actual == expected => (true, "claim field matches expected value"),
            Some(_) => (false, "claim field differs from expected value"),
            None => (false, "claim field is missing"),
        },
        ValidationRule::ClaimHashEquals { expected_hash } => {
            let actual_hash = blake3_hex(&canonical_json_bytes(claim));
            if &actual_hash == expected_hash {
                (true, "claim hash matches expected value")
            } else {
                (false, "claim hash differs from expected value")
            }
        }
        ValidationRule::EvidenceExists { .. } => (
            item.is_some(),
            if item.is_some() {
                "evidence is present"
            } else {
                "evidence is missing"
            },
        ),
        ValidationRule::EvidenceFieldEquals {
            field, expected, ..
        } => match item.and_then(|item| item.content.get(field)) {
            Some(actual) if actual == expected => (true, "field matches expected value"),
            Some(_) => (false, "field differs from expected value"),
            None => (false, "evidence or field is missing"),
        },
        ValidationRule::EvidenceHashEquals { expected_hash, .. } => match item {
            Some(item) if &item.content_hash == expected_hash => (true, "evidence hash matches"),
            Some(_) => (false, "evidence hash differs from expected value"),
            None => (false, "evidence is missing"),
        },
    };
    RuleResult {
        passed,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn case() -> ValidationCase {
        let content = json!({"result": "pass", "score": 7});
        let content_hash = blake3_hex(&canonical_json_bytes(&content));
        ValidationCase {
            case_id: "claim-1".into(),
            claim: json!({"statement": "the run passed"}),
            evidence: vec![EvidenceItem {
                evidence_id: "run-1".into(),
                content,
                content_hash: content_hash.clone(),
            }],
            rules: BTreeMap::from([
                (
                    "evidence-present".into(),
                    ValidationRule::EvidenceExists {
                        evidence_id: "run-1".into(),
                    },
                ),
                (
                    "result-passed".into(),
                    ValidationRule::EvidenceFieldEquals {
                        evidence_id: "run-1".into(),
                        field: "result".into(),
                        expected: json!("pass"),
                    },
                ),
                (
                    "evidence-hash".into(),
                    ValidationRule::EvidenceHashEquals {
                        evidence_id: "run-1".into(),
                        expected_hash: content_hash,
                    },
                ),
            ]),
        }
    }

    #[test]
    fn deterministic_validation_produces_a_verifiable_signed_verdict() {
        let case = case();
        let validator = Identity::generate("validator-1");
        let first = validate_and_sign(&case, &validator).unwrap();
        let second = validate_and_sign(&case, &validator).unwrap();
        assert_eq!(first, second);
        assert!(first.passed);
        verify_verdict(&first, &case, &validator.public_key_hex()).unwrap();
    }

    #[test]
    fn rejects_tampered_case_evidence_and_verdict() {
        let original = case();
        let validator = Identity::generate("validator-1");
        let mut verdict = validate_and_sign(&original, &validator).unwrap();
        let mut changed = original.clone();
        changed.evidence[0].content["result"] = json!("fail");
        changed.evidence[0].content_hash =
            blake3_hex(&canonical_json_bytes(&changed.evidence[0].content));
        assert!(matches!(
            verify_verdict(&verdict, &changed, &validator.public_key_hex()),
            Err(ValidatorError::CaseMismatch)
        ));
        verdict.passed = false;
        assert!(verify_verdict(&verdict, &original, &validator.public_key_hex()).is_err());
        let mut malformed = original;
        malformed.evidence[0].content_hash = "00".repeat(32);
        assert!(matches!(
            validate_and_sign(&malformed, &validator),
            Err(ValidatorError::InvalidCase(_))
        ));
    }
}
