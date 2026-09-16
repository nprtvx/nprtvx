# NeonMonkey

NeonMonkey is being rebuilt as a privacy-first messaging service. The release is being delivered in pieces so each security-sensitive layer can be reviewed before the final release.

## Rust application

The Rust/Axum application and Rust/WASM browser client are built from this
workspace:

```

The CI workflow runs formatting, Clippy with warnings denied, workspace tests,
workspace checks, the browser WASM build, a production Docker image build, and
a RustSec advisory scan on pushes and pull requests.
crates/core    shared typed identities, message envelopes, and X25519 + ChaCha20-Poly1305 API
crates/server  Axum/Tokio production API and static-file server
crates/web     Rust/WASM browser UI, API transport, and static browser shell
```

The container runs the server as a non-root `neonmonkey` user and exposes a
`/health`-based container healthcheck. Docker Compose also waits for PostgreSQL
and checks the application health endpoint before reporting the stack healthy.

Run the Rust foundation with:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo run -p neonmonkey-server
curl http://127.0.0.1:8090/health
curl http://127.0.0.1:8090/ready
```

Build the browser assets (requires `wasm32-unknown-unknown` and
`wasm-bindgen-cli`):

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
crates/web/build.sh target/neonmonkey-web
STATIC_DIR=target/neonmonkey-web cargo run -p neonmonkey-server
```

The Rust server uses PostgreSQL when `DATABASE_URL` is configured and retains a
small in-memory cache for fast reads. Sessions, identities, and encrypted direct
messages are persisted using the existing schema. Expired sessions and messages
are removed during startup and by the periodic runtime cleanup task. Redis is
reported by `/health` but is not required for local development.

The shared crypto is a conservative migration seam, not a claim of a complete
secure messenger. It uses X25519 key agreement and authenticated
ChaCha20-Poly1305, but does not implement audited X3DH, pre-key bundles, replay
protection, or the Double Ratchet. Those protocol pieces require design,
independent review, and interoperability tests before replacing the current
application. The browser client is compiled to WebAssembly and served directly by Axum.

## Run

Requires Rust 1.98 or newer and Cargo.

```bash
cargo run -p neonmonkey-server
```

Open http://localhost:8090.

The browser interface is a Rust/WASM application. `crates/web/build.sh` emits
`index.html`, `style.css`, the wasm-bindgen JavaScript loader, and the WASM
binary into a directory that Axum serves with `STATIC_DIR`.

## Current piece: username and password accounts

The account flow uses a username and password without email or phone signup:

- The browser generates an ECDH identity key pair.
- The server receives the public key and an account ID derived from it.
- The private key is encrypted in the browser with the account password before upload.
- The server stores a salted PBKDF2 password hash and the encrypted recovery bundle.
- The password is never stored or returned by the server.

The current browser client still base64-encodes message text for the existing
message API shape. This is a migration-compatible transport placeholder and
must not be treated as end-to-end encrypted. Production release remains blocked
on a versioned, independently reviewed client cryptographic protocol.

The current version-1 migration contract and its threat model are documented in
[`PROTOCOL_V1.md`](PROTOCOL_V1.md) and [`THREAT_MODEL.md`](THREAT_MODEL.md).

## Release pieces and status

1. Username and password identity accounts (implemented)
2. Encrypted one-to-one messaging (server envelope only; browser encryption pending)
3. Encrypted group conversations (not implemented)
4. Disappearing messages (expiry metadata, relay filtering, and periodic cleanup implemented)
5. Encrypted attachments (not implemented)
6. Emoji and privacy-preserving GIF integrations (not implemented)
7. iOS and Android clients (not implemented)
8. Security review and final release (blocked on the preceding items)

## Deploy on Render

This repository includes a `Dockerfile` and `render.yaml` for Render.

## Product flow

1. Open `neonmonkey.in`.
2. Choose **Create account**, enter a username, password, and display name, and NeonMonkey generates the identity automatically.
3. Choose **Log in** later with the same username and password. The password unlocks the encrypted browser-side key bundle.
4. Share the generated account ID with contacts who want to start a conversation.
5. The Home screen contains only **Messages** and **Settings**. New accounts show an empty chat list until a conversation is started.

### Blueprint deployment

1. In Render, choose **New +** → **Blueprint**.
2. Connect this repository and select the branch to deploy.
3. Render reads `render.yaml`, builds the Docker image, and deploys the web service.

### Manual web service settings

If creating the service manually, use:

| Setting | Value |
| --- | --- |
| Environment | Docker |
| Dockerfile path | `./Dockerfile` |
| Docker context | `.` |
| Instance type | Free |
| Health check path | `/health` |

Do not set `PORT` manually. Render provides it automatically, and the Rust server uses it to bind the container.

Optional GIF proxy configuration:

```text
GIF_PROVIDER_URL=<your provider search endpoint>
GIF_PROVIDER_KEY=<server-side provider key>
```

The browser never receives the provider key. Without these variables, emoji remains available and GIF search returns no results.

For local HTTP development, the session cookie does not use the `Secure`
attribute by default. Set `NEONMONKEY_COOKIE_SECURE=true` whenever the service
is behind HTTPS.

## Optional PostgreSQL persistence

The app uses in-memory storage when no database URL is configured, so local development
still starts with only `cargo run -p neonmonkey-server`. To persist identities, sessions, messages,
attachments, groups, group members, and group messages across restarts, set either
`DATABASE_URL` before starting the app. `DATABASE_URL` may use Render's
`postgres://...` format.

The normalized schema is in `db/schema.sql` and is applied automatically when a database
URL is present. Statements are tracked in the `schema_migrations` table so restarts do
not reapply completed schema steps. For a Render Blueprint, attach a PostgreSQL database
and expose its connection string as `DATABASE_URL`.

The message table includes indexes for recipient history, conversation ordering,
message-id idempotency, and expiry cleanup.
Identity uniqueness is enforced by PostgreSQL for both normalized account IDs
and usernames, including concurrent registration attempts.

### Docker Compose PostgreSQL

Docker Compose must be installed separately from the Docker Engine. Verify it with:

```bash
docker compose version
```

On Linux, install the Compose plugin using your distribution's Docker documentation,
or install the standalone `docker-compose` package. Then use the matching command:

```bash
docker compose up --build
```

For the standalone binary, use:

```bash
docker-compose up --build
```

NeonMonkey is available at http://localhost:8090. PostgreSQL data is stored in the
`neonmonkey-postgres` Docker volume and survives container restarts. To stop the
containers without deleting data:

```bash
docker compose down
```

Use `docker-compose down` when using the standalone binary.

Set `POSTGRES_DB`, `POSTGRES_USER`, `POSTGRES_PASSWORD`, or `APP_PORT` in a `.env`
file to override the development defaults. Do not use the default password in a
public deployment.

The app container receives the PostgreSQL username and password in its
`DATABASE_URL`; no separate `PGPASSWORD` setting is required.

The application still needs an independent cryptographic audit before the custom browser
protocol should be considered production-grade. Native iOS and Android clients remain
separate client projects; the shared protocol and installable PWA are included here.
