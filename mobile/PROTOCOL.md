# NeonMonkey mobile protocol

The repository's version-1 migration contract is defined in
[`../PROTOCOL_V1.md`](../PROTOCOL_V1.md), with security assumptions and release
gates in [`../THREAT_MODEL.md`](../THREAT_MODEL.md).

Native clients must not claim interoperability or end-to-end encryption yet.
The current contract records the implemented server envelope and shared-core
crypto seam, while the browser still submits placeholder base64 payloads.
The former P-256/AES-GCM proposal has been removed from the active contract;
the shared core uses X25519 and ChaCha20-Poly1305.

Native implementation is blocked until the excluded protocol pieces are
specified and reviewed: authenticated pre-key setup, forward-secure ratchets,
replay handling, key erasure, secure recovery, groups, attachments, and
canonical interoperability vectors. Native clients must reject unknown
protocol versions rather than silently falling back.

Native clients must use platform secure storage for private keys and recovery
state. They must not log private keys, recovery phrases, plaintext messages, or
decrypted attachments. No native client should ship before independent
cryptographic review and interoperability tests pass.
