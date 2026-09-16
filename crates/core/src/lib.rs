//! Shared protocol types and deliberately small cryptographic building blocks.
//!
//! This crate does **not** claim to implement audited X3DH or the Double
//! Ratchet. It currently provides authenticated encryption with an X25519
//! shared secret, which is a useful migration seam but is not a complete
//! messaging protocol. The production protocol still needs a security review,
//! replay protection, pre-key management, ratchet state, and key erasure.

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use x25519_dalek::{PublicKey, StaticSecret};

pub const PROTOCOL_VERSION: u16 = 1;
pub const NONCE_LENGTH: usize = 12;
pub const KEY_LENGTH: usize = 32;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct IdentityId(String);

impl IdentityId {
    pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
        let identity = Self(value.into());
        identity.validate()?;
        Ok(identity)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        if self.0.is_empty() || self.0.len() > 128 {
            return Err(ModelError::InvalidIdentityId);
        }

        Ok(())
    }
}

pub fn validate_protocol_version(version: u16) -> Result<(), ModelError> {
    if version != PROTOCOL_VERSION {
        return Err(ModelError::UnsupportedProtocolVersion(version));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicIdentity {
    pub account_id: IdentityId,
    pub display_name: String,
    /// An X25519 public key encoded as exactly 32 JSON bytes.
    pub identity_key: Vec<u8>,
}

impl PublicIdentity {
    pub fn new(
        account_id: IdentityId,
        display_name: impl Into<String>,
        identity_key: Vec<u8>,
    ) -> Result<Self, ModelError> {
        account_id.validate()?;
        if identity_key.len() != KEY_LENGTH {
            return Err(ModelError::InvalidKeyLength);
        }
        let display_name = display_name.into();
        if display_name.trim().is_empty() || display_name.len() > 128 {
            return Err(ModelError::InvalidDisplayName);
        }
        Ok(Self {
            account_id,
            display_name,
            identity_key,
        })
    }

    pub fn identity_key_array(&self) -> Result<[u8; KEY_LENGTH], ModelError> {
        self.identity_key
            .as_slice()
            .try_into()
            .map_err(|_| ModelError::InvalidKeyLength)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MessageEnvelope {
    pub message_id: String,
    pub sender: IdentityId,
    pub recipient: IdentityId,
    pub created_at_ms: u64,
    pub protocol_version: u16,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub associated_data: Vec<u8>,
}

impl MessageEnvelope {
    pub fn new(
        message_id: impl Into<String>,
        sender: IdentityId,
        recipient: IdentityId,
        payload: EncryptedPayload,
        associated_data: Vec<u8>,
    ) -> Result<Self, ModelError> {
        let message_id = message_id.into();
        sender.validate()?;
        recipient.validate()?;
        if message_id.is_empty() || message_id.len() > 256 {
            return Err(ModelError::InvalidMessageId);
        }
        if payload.nonce.len() != NONCE_LENGTH || payload.ciphertext.len() < 16 {
            return Err(ModelError::InvalidCiphertext);
        }
        Ok(Self {
            message_id,
            sender,
            recipient,
            created_at_ms: now_ms(),
            protocol_version: PROTOCOL_VERSION,
            nonce: payload.nonce,
            ciphertext: payload.ciphertext,
            associated_data,
        })
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        self.sender.validate()?;
        self.recipient.validate()?;
        if self.message_id.is_empty() || self.message_id.len() > 256 {
            return Err(ModelError::InvalidMessageId);
        }

        validate_protocol_version(self.protocol_version)?;
        if self.nonce.len() != NONCE_LENGTH || self.ciphertext.len() < 16 {
            return Err(ModelError::InvalidCiphertext);
        }
        Ok(())
    }

    pub fn protocol_version(&self) -> u16 {
        self.protocol_version
    }

    fn encrypted_payload(&self) -> EncryptedPayload {
        EncryptedPayload {
            nonce: self.nonce.clone(),
            ciphertext: self.ciphertext.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedPayload {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("identity id must be 1-128 characters")]
    InvalidIdentityId,
    #[error("display name must be 1-128 characters")]
    InvalidDisplayName,
    #[error("key must be exactly 32 bytes")]
    InvalidKeyLength,
    #[error("message id must be 1-256 characters")]
    InvalidMessageId,
    #[error("message has an invalid nonce, protocol version, or ciphertext")]
    InvalidCiphertext,
    #[error("unsupported protocol version: {0}")]
    UnsupportedProtocolVersion(u16),
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("authenticated encryption failed")]
    Encryption,
    #[error("authenticated decryption failed")]
    Decryption,
    #[error("invalid key material")]
    InvalidKey,
    #[error("invalid message envelope: {0}")]
    InvalidEnvelope(#[from] ModelError),
}

/// A long-term X25519 identity key. Keep the secret in platform secure storage
/// in clients; this type intentionally does not implement `Serialize`.
pub struct IdentityKeypair {
    secret: StaticSecret,
    public: PublicKey,
}

impl IdentityKeypair {
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    pub fn from_secret_bytes(bytes: [u8; KEY_LENGTH]) -> Self {
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    pub fn public_key(&self) -> [u8; KEY_LENGTH] {
        self.public.to_bytes()
    }

    pub fn secret_bytes(&self) -> [u8; KEY_LENGTH] {
        self.secret.to_bytes()
    }

    pub fn secret(&self) -> &StaticSecret {
        &self.secret
    }
}

pub fn derive_shared_key(
    local_secret: &StaticSecret,
    remote_public: &[u8; KEY_LENGTH],
) -> [u8; KEY_LENGTH] {
    let remote_public = PublicKey::from(*remote_public);
    let shared = local_secret.diffie_hellman(&remote_public);
    let digest = Sha256::digest(shared.as_bytes());
    digest.into()
}

pub fn encrypt(
    key: &[u8; KEY_LENGTH],
    plaintext: &[u8],
    associated_data: &[u8],
) -> Result<EncryptedPayload, CryptoError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce = vec![0_u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: associated_data,
            },
        )
        .map_err(|_| CryptoError::Encryption)?;
    Ok(EncryptedPayload { nonce, ciphertext })
}

pub fn decrypt(
    key: &[u8; KEY_LENGTH],
    payload: &EncryptedPayload,
    associated_data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if payload.nonce.len() != NONCE_LENGTH {
        return Err(CryptoError::Decryption);
    }
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(
            Nonce::from_slice(&payload.nonce),
            Payload {
                msg: &payload.ciphertext,
                aad: associated_data,
            },
        )
        .map_err(|_| CryptoError::Decryption)
}

pub fn encrypt_for(
    sender: &IdentityKeypair,
    recipient_public: &[u8; KEY_LENGTH],
    plaintext: &[u8],
    associated_data: &[u8],
) -> Result<EncryptedPayload, CryptoError> {
    let key = derive_shared_key(sender.secret(), recipient_public);
    encrypt(&key, plaintext, associated_data)
}

pub fn decrypt_from(
    recipient: &IdentityKeypair,
    sender_public: &[u8; KEY_LENGTH],
    envelope: &MessageEnvelope,
) -> Result<Vec<u8>, CryptoError> {
    envelope.validate()?;
    let key = derive_shared_key(recipient.secret(), sender_public);
    decrypt(
        &key,
        &envelope.encrypted_payload(),
        &envelope.associated_data,
    )
}

pub fn new_message_id() -> String {
    let mut bytes = [0_u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x25519_chacha_round_trip() {
        let alice = IdentityKeypair::generate();
        let bob = IdentityKeypair::generate();
        let aad = b"conversation:demo";
        let payload = encrypt_for(&alice, &bob.public_key(), b"hello", aad).unwrap();
        let envelope = MessageEnvelope::new(
            new_message_id(),
            IdentityId::new("alice").unwrap(),
            IdentityId::new("bob").unwrap(),
            payload,
            aad.to_vec(),
        )
        .unwrap();
        assert_eq!(
            decrypt_from(&bob, &alice.public_key(), &envelope).unwrap(),
            b"hello"
        );
    }

    #[test]
    fn tampering_is_rejected() {
        let key = [7_u8; KEY_LENGTH];
        let mut payload = encrypt(&key, b"hello", b"aad").unwrap();
        payload.ciphertext[0] ^= 1;
        assert!(decrypt(&key, &payload, b"aad").is_err());
    }

    #[test]
    fn unknown_protocol_versions_are_rejected() {
        let alice = IdentityKeypair::generate();
        let bob = IdentityKeypair::generate();
        let payload = encrypt_for(&alice, &bob.public_key(), b"hello", b"aad").unwrap();
        let mut envelope = MessageEnvelope::new(
            new_message_id(),
            IdentityId::new("alice").unwrap(),
            IdentityId::new("bob").unwrap(),
            payload,
            b"aad".to_vec(),
        )
        .unwrap();
        envelope.protocol_version = PROTOCOL_VERSION + 1;
        assert!(matches!(
            envelope.validate(),
            Err(ModelError::UnsupportedProtocolVersion(_))
        ));
    }
}
