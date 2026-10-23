use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("invalid public key: {0}")]
    InvalidPublicKey(String),
    #[error("invalid signature encoding: {0}")]
    InvalidSignatureEncoding(String),
    #[error("invalid signature: {0}")]
    InvalidSignature(String),
    #[error("signature verification failed")]
    VerificationFailed,
}

#[derive(Clone)]
pub struct Identity {
    pub agent_id: String,
    signing_key: SigningKey,
}

impl Identity {
    pub fn generate(agent_id: impl Into<String>) -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self {
            agent_id: agent_id.into(),
            signing_key,
        }
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.verifying_key().to_bytes())
    }

    pub fn sign(&self, bytes: &[u8]) -> String {
        hex::encode(self.signing_key.sign(bytes).to_bytes())
    }
}

pub fn verify(
    public_key_hex: &str,
    bytes: &[u8],
    signature_hex: &str,
) -> Result<(), IdentityError> {
    let key_bytes =
        hex::decode(public_key_hex).map_err(|e| IdentityError::InvalidPublicKey(e.to_string()))?;
    let key_arr: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| IdentityError::InvalidPublicKey("expected 32 bytes".into()))?;
    let key = VerifyingKey::from_bytes(&key_arr)
        .map_err(|e| IdentityError::InvalidPublicKey(e.to_string()))?;
    let sig_bytes = hex::decode(signature_hex)
        .map_err(|e| IdentityError::InvalidSignatureEncoding(e.to_string()))?;
    let signature = Signature::from_slice(&sig_bytes)
        .map_err(|e| IdentityError::InvalidSignature(e.to_string()))?;
    key.verify(bytes, &signature)
        .map_err(|_| IdentityError::VerificationFailed)
}
