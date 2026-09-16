# NeonMonkey mobile protocol

The repository currently has two incompatible protocol descriptions. Until the
versioned protocol and cryptographic review are complete, native clients must
not claim interoperability with the browser:

- The current Rust foundation uses X25519 identity keys and
  ChaCha20-Poly1305 envelopes in `crates/core`.
- The current browser is still a migration client and sends placeholder
  base64 fields; those fields are not end-to-end encrypted.
- This document's former P-256/AES-GCM design is a proposal, not an implemented
  contract.

Before native implementation begins, the protocol work must choose one
versioned contract and publish canonical test vectors for identity generation,
recovery, direct messages, groups, attachments, replay handling, expiry, and
key rotation. Native clients must reject unknown protocol versions rather than
silently falling back.

Native clients must use platform secure storage for the private key and
recovery state. They must not log private keys, recovery phrases, plaintext
messages, or decrypted attachments. No native client should ship until the
protocol has independent cryptographic review and interoperability tests.
