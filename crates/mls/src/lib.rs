//! Real MLS primitives for NeonMonkey.
//!
//! The relay must never decrypt, inspect, modify, or invent MLS messages.

pub use openmls::prelude::*;
pub use openmls_basic_credential::SignatureKeyPair;
pub use openmls_rust_crypto::OpenMlsRustCrypto;

pub const CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519;

pub struct Identity {
    pub provider: OpenMlsRustCrypto,
    pub credential: CredentialWithKey,
    pub signer: SignatureKeyPair,
}

impl Identity {
    pub fn create(name: &str) -> Result<Self, String> {
        let provider = OpenMlsRustCrypto::default();
        let credential = BasicCredential::new(name.as_bytes().to_vec());
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())
            .map_err(|error| error.to_string())?;
        let credential = CredentialWithKey {
            credential: credential.into(),
            signature_key: signer.to_public_vec().into(),
        };
        Ok(Self {
            provider,
            credential,
            signer,
        })
    }

    pub fn key_package(&self) -> Result<KeyPackage, String> {
        KeyPackage::builder()
            .build(
                CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential.clone(),
            )
            .map(|bundle| bundle.key_package().clone())
            .map_err(|error| error.to_string())
    }

    pub fn serialize_key_package(&self) -> Result<Vec<u8>, String> {
        use tls_codec::Serialize;
        let mut output = Vec::new();
        self.key_package()?
            .tls_serialize(&mut output)
            .map_err(|error| error.to_string())?;
        Ok(output)
    }
}

pub fn create_group(identity: &Identity, group_id: &[u8]) -> Result<MlsGroup, String> {
    let config = MlsGroupCreateConfig::builder()
        .ciphersuite(CIPHERSUITE)
        .build();
    MlsGroup::new_with_group_id(
        &identity.provider,
        &identity.signer,
        &config,
        GroupId::from_slice(group_id),
        identity.credential.clone(),
    )
    .map_err(|error| error.to_string())
}

pub fn serialize_message(message: &MlsMessageOut) -> Result<Vec<u8>, String> {
    use tls_codec::Serialize;
    let mut output = Vec::new();
    message
        .tls_serialize(&mut output)
        .map_err(|error| error.to_string())?;
    Ok(output)
}

pub fn parse_message(bytes: &[u8]) -> Result<MlsMessageIn, String> {
    use tls_codec::Deserialize;
    MlsMessageIn::tls_deserialize(&mut &bytes[..]).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_group_uses_the_requested_group_id() {
        let identity = Identity::create("alice").unwrap();
        let group = create_group(&identity, b"neonmonkey-test-group").unwrap();

        assert_eq!(group.group_id().as_slice(), b"neonmonkey-test-group");
    }

    #[test]
    fn alice_and_bob_can_exchange_application_messages() {
        let alice = Identity::create("alice").unwrap();
        let bob = Identity::create("bob").unwrap();
        let bob_package = bob.key_package().unwrap();
        let config = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .build();

        let mut alice_group = MlsGroup::new(
            &alice.provider,
            &alice.signer,
            &config,
            alice.credential.clone(),
        )
        .unwrap();
        let (commit, welcome, _) = alice_group
            .add_members(&alice.provider, &alice.signer, &[bob_package])
            .unwrap();
        alice_group.merge_pending_commit(&alice.provider).unwrap();

        let welcome: MlsMessageIn = welcome.into();
        let welcome = welcome.into_welcome().unwrap();
        let mut bob_group = StagedWelcome::new_from_welcome(
            &bob.provider,
            config.join_config(),
            welcome,
            Some(alice_group.export_ratchet_tree().into()),
        )
        .unwrap()
        .into_group(&bob.provider)
        .unwrap();

        let message = alice_group
            .create_message(&alice.provider, &alice.signer, b"hello")
            .unwrap();
        let message_bytes = serialize_message(&message).unwrap();
        let message = parse_message(&message_bytes).unwrap();
        let processed = bob_group
            .process_message(&bob.provider, message)
            .unwrap();
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(application) => {
                assert_eq!(application.into_bytes(), b"hello");
            }
            other => panic!("unexpected MLS content: {other:?}"),
        }

        drop(commit);
    }
}
