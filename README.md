# NeonMonkey

NeonMonkey is being migrated from its Rust prototype to a Java 21 / Spring Boot chat relay. This branch provides a small REST API foundation for anonymous-by-contact accounts, direct and group rooms, opaque encrypted message envelopes, and message expiry.

## Run locally

Requires Docker Compose:

```bash
docker compose up --build
```

The API listens on `http://localhost:8080`. PostgreSQL data is kept in the `neonmonkey-postgres` volume. The compose password is for local development only; set `DB_PASSWORD` before using a non-local environment.

## API outline

- `POST /api/auth/register` — create an account with a unique username and passphrase (12-200 characters). There is no email address or phone number field. The response includes a bearer token; store it securely because it is shown only when issued.
- `POST /api/auth/login` — obtain a new bearer token.
- `POST /api/rooms` — create a `direct` room or a `group` room. `participants` is a list of existing usernames; the authenticated user is automatically included. Groups require a title.
- `GET /api/rooms` — list rooms the account belongs to.
- `POST /api/rooms/{roomId}/messages` — send a base64-encoded opaque ciphertext and optional `expiresInSeconds` (1 second to 30 days).
- `GET /api/rooms/{roomId}/messages?limit=50` — fetch unexpired messages for a room (limit 1-100).
- `GET /health` — health check.

Example registration:

```bash
curl -X POST http://localhost:8080/api/auth/register \\
  -H 'Content-Type: application/json' \\
  -d '{"username":"alice","passphrase":"a-long-unique-passphrase"}'
```

For authenticated calls, send `Authorization: Bearer <accessToken>`. Use clients to encrypt messages before sending; this relay stores only the supplied ciphertext and cannot perform the encryption for clients.

## Privacy and security status

This is a backend MVP, not an audited secure messenger. Usernames, room membership, message sizes/timing, and expiration metadata are visible to the server. The server stores message envelopes opaquely but this branch does not yet include a client, key registration, authenticated key exchange, forward secrecy, key recovery, or a reviewed end-to-end encryption protocol. Do not send plaintext and do not claim production-grade E2EE. Use HTTPS in deployment, replace local credentials, and get independent security review before handling sensitive communications.

Accounts are username + passphrase; password hashes use BCrypt and issued bearer tokens are stored server-side only as SHA-256 hashes. Disappearing messages are deleted by a periodic database cleanup, so expiry is best-effort around the configured time and is not a guarantee against recipients saving content.

## Build

```bash
mvn test
mvn spring-boot:run
```
