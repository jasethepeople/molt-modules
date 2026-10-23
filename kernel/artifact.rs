use crate::events::{create_signed_event, Event};
use crate::hash::{blake3_hex, canonical_json_bytes};
use crate::identity::{verify, Identity, IdentityError};
use crate::store::{EventStore, StoreError};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactManifest {
    pub schema_version: u32,
    pub artifact_id: String,
    pub manifest_id: String,
    pub media_type: String,
    pub byte_length: u64,
    pub creator_id: String,
    pub created_at: String,
    pub source_event_ids: Vec<String>,
    pub parent_manifest_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedArtifactManifest {
    pub manifest: ArtifactManifest,
    pub signer_public_key: String,
    pub signature: String,
}

pub struct ArtifactInput<'a> {
    pub media_type: &'a str,
    pub bytes: &'a [u8],
    pub created_at: &'a str,
    pub source_event_ids: Vec<String>,
    pub parent_manifest_ids: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("artifact storage I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("artifact manifest serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("artifact signature is invalid: {0}")]
    Signature(#[from] IdentityError),
    #[error("invalid artifact: {0}")]
    Invalid(String),
    #[error("artifact content hash does not match its bytes")]
    ContentHashMismatch,
    #[error("immutable artifact path already exists with different contents")]
    ContentConflict,
    #[error("parent artifact manifest {0} is missing or invalid")]
    MissingParent(String),
    #[error("artifact signer does not match the requested trusted key")]
    UntrustedSigner,
    #[error("event store error: {0}")]
    Store(#[from] StoreError),
    #[error("event sequence or logical clock exhausted")]
    ClockExhausted,
}

#[derive(Debug, Clone)]
pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ArtifactError> {
        let root = path.as_ref().to_path_buf();
        fs::create_dir_all(root.join("objects"))?;
        fs::create_dir_all(root.join("manifests"))?;
        Ok(Self { root })
    }

    pub fn put(
        &self,
        signer: &Identity,
        media_type: &str,
        bytes: &[u8],
        created_at: &str,
        source_event_ids: Vec<String>,
        parent_manifest_ids: Vec<String>,
    ) -> Result<SignedArtifactManifest, ArtifactError> {
        validate_label("media type", media_type)?;
        validate_label("timestamp", created_at)?;
        if source_event_ids.iter().any(|id| !is_digest(id)) || has_duplicates(&source_event_ids) {
            return Err(ArtifactError::Invalid(
                "source event IDs must be unique 64-character lowercase BLAKE3 digests".into(),
            ));
        }
        if parent_manifest_ids.iter().any(|id| !is_digest(id))
            || has_duplicates(&parent_manifest_ids)
        {
            return Err(ArtifactError::Invalid(
                "parent manifest IDs must be unique 64-character lowercase BLAKE3 digests".into(),
            ));
        }
        for parent_id in &parent_manifest_ids {
            if self.get(parent_id, None).is_err() {
                return Err(ArtifactError::MissingParent(parent_id.clone()));
            }
        }

        let artifact_id = blake3_hex(bytes);
        let mut manifest = ArtifactManifest {
            schema_version: 1,
            artifact_id,
            manifest_id: String::new(),
            media_type: media_type.to_owned(),
            byte_length: bytes.len() as u64,
            creator_id: signer.agent_id.clone(),
            created_at: created_at.to_owned(),
            source_event_ids,
            parent_manifest_ids,
        };
        manifest.manifest_id = manifest_hash(&manifest);
        let signer_public_key = signer.public_key_hex();
        let signature = signer.sign(&canonical_json_bytes(&serde_json::to_value(&manifest)?));
        let signed = SignedArtifactManifest {
            manifest,
            signer_public_key,
            signature,
        };

        let object_path = self.object_path(&signed.manifest.artifact_id);
        if let Some(parent) = object_path.parent() {
            fs::create_dir_all(parent)?;
        }
        persist_immutable(&object_path, bytes)?;

        let manifest_path = self.manifest_path(&signed.manifest.manifest_id);
        let encoded = serde_json::to_vec(&signed)?;
        match persist_immutable(&manifest_path, &encoded) {
            Ok(()) => Ok(signed),
            Err(ArtifactError::ContentConflict) => {
                let existing: SignedArtifactManifest =
                    serde_json::from_slice(&fs::read(manifest_path)?)?;
                if existing == signed {
                    Ok(existing)
                } else {
                    Err(ArtifactError::ContentConflict)
                }
            }
            Err(error) => Err(error),
        }
    }

    pub fn put_and_record<S: EventStore>(
        &self,
        store: &mut S,
        signer: &Identity,
        input: ArtifactInput<'_>,
    ) -> Result<(SignedArtifactManifest, Event), ArtifactError> {
        let signed = self.put(
            signer,
            input.media_type,
            input.bytes,
            input.created_at,
            input.source_event_ids,
            input.parent_manifest_ids,
        )?;
        self.get(&signed.manifest.manifest_id, Some(&signer.public_key_hex()))?;
        let history = store.all()?;
        let last = history.iter().max_by_key(|event| event.sequence_number);
        let sequence_number = last
            .map(|event| event.sequence_number.checked_add(1))
            .unwrap_or(Some(0))
            .ok_or(ArtifactError::ClockExhausted)?;
        let logical_clock = last
            .map(|event| event.logical_clock.checked_add(1))
            .unwrap_or(Some(0))
            .ok_or(ArtifactError::ClockExhausted)?;
        let parents = if signed.manifest.source_event_ids.is_empty() {
            last.map(|event| vec![event.event_id.clone()])
                .unwrap_or_default()
        } else {
            signed.manifest.source_event_ids.clone()
        };
        let event = create_signed_event(
            signer,
            "ARTIFACT_MANIFEST_CREATED",
            "MOLT/0.1",
            logical_clock,
            sequence_number,
            input.created_at,
            parents,
            serde_json::json!({
                "manifest_id": signed.manifest.manifest_id,
                "artifact_id": signed.manifest.artifact_id,
                "media_type": signed.manifest.media_type,
                "byte_length": signed.manifest.byte_length,
                "parent_manifest_ids": signed.manifest.parent_manifest_ids,
            }),
            vec![signed.manifest.manifest_id.clone()],
            vec![],
        );
        store.append(event.clone())?;
        Ok((signed, event))
    }

    pub fn manifest(
        &self,
        manifest_id: &str,
        trusted_public_key: Option<&str>,
    ) -> Result<SignedArtifactManifest, ArtifactError> {
        let (signed, _) = self.get(manifest_id, trusted_public_key)?;
        Ok(signed)
    }

    pub fn get(
        &self,
        manifest_id: &str,
        trusted_public_key: Option<&str>,
    ) -> Result<(SignedArtifactManifest, Vec<u8>), ArtifactError> {
        let signed = self.read_manifest(manifest_id, trusted_public_key)?;
        let mut file = File::open(self.object_path(&signed.manifest.artifact_id))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        if bytes.len() as u64 != signed.manifest.byte_length
            || blake3_hex(&bytes) != signed.manifest.artifact_id
        {
            return Err(ArtifactError::ContentHashMismatch);
        }
        Ok((signed, bytes))
    }

    fn read_manifest(
        &self,
        manifest_id: &str,
        trusted_public_key: Option<&str>,
    ) -> Result<SignedArtifactManifest, ArtifactError> {
        if !is_digest(manifest_id) {
            return Err(ArtifactError::Invalid(
                "manifest ID must be a 64-character lowercase BLAKE3 digest".into(),
            ));
        }
        let signed: SignedArtifactManifest =
            serde_json::from_slice(&fs::read(self.manifest_path(manifest_id))?)?;
        validate_manifest(&signed.manifest)?;
        if signed.manifest.manifest_id != manifest_id
            || signed.manifest.manifest_id != manifest_hash(&signed.manifest)
        {
            return Err(ArtifactError::Invalid(
                "manifest ID does not match signed manifest".into(),
            ));
        }
        if let Some(trusted) = trusted_public_key {
            if trusted != signed.signer_public_key {
                return Err(ArtifactError::UntrustedSigner);
            }
        }
        verify(
            &signed.signer_public_key,
            &canonical_json_bytes(&serde_json::to_value(&signed.manifest)?),
            &signed.signature,
        )?;
        Ok(signed)
    }

    fn object_path(&self, artifact_id: &str) -> PathBuf {
        self.root
            .join("objects")
            .join(&artifact_id[..2])
            .join(&artifact_id[2..])
    }

    fn manifest_path(&self, manifest_id: &str) -> PathBuf {
        self.root
            .join("manifests")
            .join(format!("{manifest_id}.json"))
    }
}

fn validate_manifest(manifest: &ArtifactManifest) -> Result<(), ArtifactError> {
    if manifest.schema_version != 1
        || !is_digest(&manifest.artifact_id)
        || !is_digest(&manifest.manifest_id)
        || manifest.creator_id.trim().is_empty()
        || manifest.media_type.trim().is_empty()
        || manifest.created_at.trim().is_empty()
        || manifest.source_event_ids.iter().any(|id| !is_digest(id))
        || manifest.parent_manifest_ids.iter().any(|id| !is_digest(id))
        || has_duplicates(&manifest.source_event_ids)
        || has_duplicates(&manifest.parent_manifest_ids)
    {
        return Err(ArtifactError::Invalid(
            "manifest fields or digest formats are invalid".into(),
        ));
    }
    Ok(())
}

fn validate_label(name: &str, value: &str) -> Result<(), ArtifactError> {
    if value.trim().is_empty() {
        return Err(ArtifactError::Invalid(format!("{name} must be non-empty")));
    }
    Ok(())
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

fn manifest_hash(manifest: &ArtifactManifest) -> String {
    let mut content = manifest.clone();
    content.manifest_id.clear();
    blake3_hex(&canonical_json_bytes(
        &serde_json::to_value(content).expect("serializable artifact manifest"),
    ))
}

fn persist_immutable(path: &Path, bytes: &[u8]) -> Result<(), ArtifactError> {
    if path.exists() {
        let existing = fs::read(path)?;
        if existing == bytes {
            return Ok(());
        }
        return Err(ArtifactError::ContentConflict);
    }
    let parent = path
        .parent()
        .ok_or_else(|| ArtifactError::Invalid("artifact path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".tmp-{}-{}", std::process::id(), unique_suffix()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    match fs::hard_link(&temporary, path) {
        Ok(()) => {
            fs::remove_file(&temporary)?;
            File::open(parent)?.sync_all()?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(&temporary);
            if fs::read(path)? == bytes {
                Ok(())
            } else {
                Err(ArtifactError::ContentConflict)
            }
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(ArtifactError::Io(error))
        }
    }
}

fn unique_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "molt-artifact-test-{}-{}",
            std::process::id(),
            unique_suffix()
        ))
    }

    #[test]
    fn content_addressed_artifact_has_signed_provenance_and_verified_bytes() {
        let directory = temp_dir();
        let store = ArtifactStore::open(&directory).unwrap();
        let identity = Identity::generate("builder-1");
        let signed = store
            .put(
                &identity,
                "text/plain",
                b"deterministic build output\n",
                "2026-10-22T00:00:00Z",
                vec!["a1".repeat(32)],
                vec![],
            )
            .unwrap();
        let (loaded, bytes) = store
            .get(
                &signed.manifest.manifest_id,
                Some(&identity.public_key_hex()),
            )
            .unwrap();
        assert_eq!(loaded, signed);
        assert_eq!(bytes, b"deterministic build output\n");
        let repeated = store
            .put(
                &identity,
                "text/plain",
                b"deterministic build output\n",
                "2026-10-22T00:00:00Z",
                vec!["a1".repeat(32)],
                vec![],
            )
            .unwrap();
        assert_eq!(repeated, signed);
        assert!(matches!(
            store.get(&signed.manifest.manifest_id, Some("00".repeat(32).as_str())),
            Err(ArtifactError::UntrustedSigner)
        ));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn artifact_creation_is_linked_to_the_signed_event_history() {
        let directory = temp_dir();
        let artifacts = ArtifactStore::open(&directory).unwrap();
        let identity = Identity::generate("builder-1");
        let source = create_signed_event(
            &identity,
            "BUILD_COMPLETED",
            "MOLT/0.1",
            0,
            0,
            "2026-10-22T00:00:00Z",
            vec![],
            serde_json::json!({"build_id":"build-1"}),
            vec![],
            vec![],
        );
        let mut events = crate::store::MemoryEventStore::new();
        events.append(source.clone()).unwrap();
        let (manifest, receipt) = artifacts
            .put_and_record(
                &mut events,
                &identity,
                ArtifactInput {
                    media_type: "application/octet-stream",
                    bytes: b"build output",
                    created_at: "2026-10-22T00:00:01Z",
                    source_event_ids: vec![source.event_id.clone()],
                    parent_manifest_ids: vec![],
                },
            )
            .unwrap();
        assert_eq!(receipt.event_type, "ARTIFACT_MANIFEST_CREATED");
        assert_eq!(receipt.parents, vec![source.event_id]);
        assert_eq!(
            receipt.payload["manifest_id"],
            manifest.manifest.manifest_id
        );
        receipt.verify(&identity.public_key_hex()).unwrap();
        assert_eq!(events.all().unwrap().len(), 2);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn parent_manifests_are_required_and_object_tampering_is_detected() {
        let directory = temp_dir();
        let store = ArtifactStore::open(&directory).unwrap();
        let identity = Identity::generate("builder-1");
        let missing_parent = "ab".repeat(32);
        assert!(matches!(
            store.put(
                &identity,
                "application/octet-stream",
                b"child",
                "2026-10-22T00:00:00Z",
                vec![],
                vec![missing_parent.clone()],
            ),
            Err(ArtifactError::MissingParent(parent)) if parent == missing_parent
        ));
        let artifact = store
            .put(
                &identity,
                "application/octet-stream",
                b"source",
                "2026-10-22T00:00:00Z",
                vec![],
                vec![],
            )
            .unwrap();
        let path = store.object_path(&artifact.manifest.artifact_id);
        fs::write(path, b"altered").unwrap();
        assert!(matches!(
            store.get(&artifact.manifest.manifest_id, None),
            Err(ArtifactError::ContentHashMismatch)
        ));
        fs::remove_dir_all(directory).unwrap();
    }
}
