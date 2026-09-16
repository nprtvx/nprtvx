# NeonMonkey threat model

## Scope

This threat model covers the current account, session, direct-message relay,
PostgreSQL persistence, and browser/WASM migration client. It distinguishes
properties implemented today from properties required before a production
privacy claim.

## Assets

- Password verifiers, session tokens, identity public keys, and recovery data.
- Message payloads, participants, timestamps, expiry metadata, and account
  relationships.
- Future client private keys and decrypted message or attachment content.
- Database backups, deployment credentials, and provider credentials.

## Actors and assumptions

| Actor | Capability or assumption |
|---|---|
| Unauthenticated internet client | Can send malformed, oversized, replayed, and rate-limited requests |
| Authenticated user | May access only their own session and authorized direct-message paths |
| Malicious account | May attempt account enumeration, credential guessing, and message-ID abuse |
| Compromised server or database operator | Can inspect all data currently submitted to persistence |
| Network attacker | HTTPS/TLS and a trusted production ingress are required |
| Browser compromise | Out of scope; XSS or a compromised device defeats client-side secrecy |

The current deployment does not provide confidentiality from a compromised
server because the browser migration client submits base64-encoded plaintext
message content.

## Security goals currently implemented

- Passwords are stored as salted PBKDF2 verifiers, not plaintext.
- Sessions expire, can be revoked by logout, and use HttpOnly/SameSite cookies.
- PostgreSQL is authoritative for sessions and durable identity/message reads
  when configured.
- State-changing requests enforce same-origin checks when an `Origin` header
  is present.
- Authentication attempts have source rate limits and temporary account
  lockout.
- Public identity responses omit password hashes and recovery bundles.
- Message IDs are bounded and persisted idempotently; expiry is enforced on
  reads and cleanup.
- Unknown protocol versions and malformed binary payloads are rejected.

## Security properties not yet achieved

- End-to-end confidentiality or sender authentication against the server.
- Forward secrecy, post-compromise security, replay protection, or key erasure.
- Secure browser private-key storage and password-based recovery.
- Distributed rate limiting and coordinated abuse/audit telemetry.
- Independent review of the cryptographic construction and implementation.

## Main threats and mitigations

| Threat | Current mitigation | Remaining gap |
|---|---|---|
| Credential guessing | PBKDF2, source rate limit, account lockout | Rate limits are process-local |
| Session theft | HttpOnly/SameSite cookies, expiry, logout revocation | TLS and ingress configuration remain deployment responsibilities |
| Cross-origin state changes | Origin/Host validation | Requests without `Origin` are allowed for compatibility |
| Payload abuse | Body, base64, nonce, ciphertext, expiry, and version bounds | Broader resource quotas and distributed controls are pending |
| Duplicate message submission | UUID validation and database `ON CONFLICT` handling | Replay semantics are not cryptographic replay protection |
| Stale multi-instance state | PostgreSQL-authoritative session, identity, conversation, and message paths | Cache invalidation and distributed rate limiting remain |
| Database disclosure | Operational backup guidance and recovery procedures | Current browser payloads are readable by the server/database |
| Protocol downgrade | Explicit version rejection | A complete negotiated future protocol is not implemented |

## Release decision

The current service is suitable only as a migration seam and development
baseline. A production privacy release requires a reviewed protocol revision,
real client encryption, secure key lifecycle, replay/ratchet behavior,
interoperability vectors, integration tests, operational controls, and
independent cryptographic review. No README, API, or client copy should claim
audited end-to-end encryption before those gates pass.
