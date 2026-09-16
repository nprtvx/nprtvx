# NeonMonkey protocol version 1

This document is the version-1 migration contract for the currently shipped
Rust server and shared core. It is deliberately narrower than a production
secure-messaging protocol. A client must not describe this contract as
end-to-end encryption.

## Compatibility

- `protocolVersion` is the integer `1`.
- Clients must send the version on registration and direct-message writes.
- Servers reject unknown versions; clients must reject unknown versions rather
  than silently downgrade.
- The browser and server use JSON over same-origin HTTPS in production.
- The server stores the direct-message envelope and can read the payload
  submitted by the current browser client. The current browser payload is
  base64-encoded application plaintext, not ciphertext produced by the shared
  core.

## Identity and account contract

An account has:

| Field | Contract |
|---|---|
| `accountId` | Exactly 32 ASCII hexadecimal characters after normalization |
| `username` | 3–24 lowercase ASCII letters, digits, or `_` |
| `displayName` | Non-empty, at most 128 characters |
| `publicKey` | Base64 encoding of exactly 32 decoded bytes |
| `recoveryBundle` | Base64 data, bounded to 1,000,000 decoded bytes |

The PostgreSQL schema enforces uniqueness for normalized account IDs and
case-folded usernames. Recovery bundles and password hashes are never included
in public identity responses. The server stores a PBKDF2 password hash; it
does not receive a password-derived recovery key in the current migration
client.

The shared core's `IdentityKeypair` is an X25519 keypair. The current browser
identity generator is not yet wired to that type and must not be treated as an
interoperable identity implementation.

## Direct-message envelope

The server API accepts:

```json
{
  "protocolVersion": 1,
  "messageId": "uuid",
  "iv": "base64",
  "ciphertext": "base64",
  "expiresInSeconds": 3600
}
```

- `messageId` is optional on input; when supplied it must be a UUID.
- `iv` decodes to exactly 12 bytes.
- `ciphertext` decodes to at least 16 bytes and at most 12,000 bytes.
- Expiry is optional or between 1 second and 30 days.
- Repeating the same message ID for the same sender and recipient is
  idempotent. Reusing it for another sender or recipient is a conflict.
- Expired messages are not relayed and are removed by startup and periodic
  cleanup.

The shared-core envelope uses:

- X25519 key agreement;
- SHA-256 of the shared secret as the ChaCha20-Poly1305 key;
- a 12-byte random nonce;
- authenticated associated data:
  `neonmonkey:v1:direct:<message-id>:<sender>:<recipient>:<created-at-ms>`.

The associated-data helper is available for coordinated client migration.
The API server does not currently verify that submitted fields were produced
by this construction.

## Server trust boundary

The server is trusted to authenticate accounts, authorize sender/recipient
relationships, enforce limits and expiry, persist envelopes, and protect
credentials and recovery data. It can observe account metadata, timestamps,
participants, expiry values, request source information, and all payload data
submitted by the current browser client.

The server is not assumed trustworthy for a future E2EE release. A compliant
future client must encrypt before submission, authenticate the envelope with
the canonical associated data, and avoid sending plaintext to server APIs.

## Deliberate version-1 exclusions

These are not implemented by this contract:

- X3DH or another authenticated pre-key handshake;
- Double Ratchet or equivalent forward-secure sessions;
- replay windows, receive counters, or key erasure;
- device lists, multi-device synchronization, or key transparency;
- groups, encrypted attachments, or native-client interoperability;
- offline queues and delivery/read receipts;
- audited browser key generation and password-protected local recovery.

Adding any excluded feature requires a new protocol revision or an explicitly
reviewed extension, canonical test vectors, downgrade tests, and independent
cryptographic review.

## Interoperability gate

The following version-1 structural vectors are normative:

| Case | Input | Expected result |
|---|---|---|
| Protocol | `protocolVersion = 1` | Accepted |
| Downgrade | `protocolVersion = 0` | Rejected as unsupported |
| Sender | `alice` | Valid identity ID |
| Recipient | `bob` | Valid identity ID |
| Message ID | `message-1` | Valid non-empty message ID |
| Created time | `123` | Used verbatim in associated data |
| Associated data | `neonmonkey:v1:direct:message-1:alice:bob:123` | Exact UTF-8 bytes |
| Nonce | 12 decoded bytes | Accepted |
| Ciphertext | 16 decoded bytes or more | Accepted |
| Empty associated data | Empty byte string | Rejected |
| Timestamp mutation | Change `123` to `124` without re-encrypting | Rejected by canonical validation |

The shared-core tests in `crates/core/src/lib.rs` execute these vectors for
version checks, associated-data construction, low-order key rejection,
authenticated round trips, tampering, and timestamp mutation. They are
structural vectors, not a claim that the current browser transport conforms:
the browser still submits base64-encoded plaintext placeholders. Future
clients must add fixed key-agreement and ciphertext vectors before claiming
cross-platform cryptographic interoperability.
