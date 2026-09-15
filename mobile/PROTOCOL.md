# NeonMonkey mobile protocol

The web client and future iOS/Android clients use the same JSON API and cryptographic contract:

- Identity: client-generated P-256 ECDH key pair.
- Account ID: first 32 hexadecimal characters of SHA-256(public JWK JSON).
- Recovery: client encrypts the private-key bundle with AES-GCM using a PBKDF2-derived recovery key.
- Direct messages: ECDH-derived AES-GCM key; only `{iv, ciphertext}` crosses the API.
- Group messages: a random AES-GCM group key, wrapped separately for each member with an ECDH-derived AES-GCM key.
- Attachments: encrypted in the client before upload with the same conversation key.
- Expiry: `expiresInSeconds` is metadata for relay cleanup and client display.

Native clients must use platform secure storage for the private key and recovery state. They must not log private keys, recovery phrases, plaintext messages, or decrypted attachments.
