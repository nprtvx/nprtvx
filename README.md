# NeonMonkey

NeonMonkey is a Rust-first private messenger foundation built around **OpenMLS**.

> This repository is an implementation in progress. Cryptography is provided by OpenMLS, not by a custom “envelope” implementation. Native iOS/Android/desktop packaging, authentication hardening, delivery receipts, and an independent security audit remain outside the initial foundation.

## Architecture

- `crates/mls` — OpenMLS identity, KeyPackage, group, application-message, add-member, and Welcome primitives
- `crates/server` — Axum relay and PostgreSQL-backed delivery service
- `crates/web` — Rust/WASM cross-platform web client
- `crates/client` — SQLite state and API abstractions shared by native clients
- `db/schema.sql` — PostgreSQL schema
- `migrations/` — SQLite migrations

Server-side storage:

- PostgreSQL is the production relay database.
- SQLite is for client state and local data; it is not the network authority.

## Technology

- Rust
- OpenMLS 0.9
- OpenMLS Rust crypto provider
- Axum/Tokio
- SQLx + PostgreSQL
- SQLx + SQLite
- WebAssembly for the browser client

## Run locally

Start the server and PostgreSQL together:

```bash
docker compose up --build
```

The relay is available at `http://localhost:8090`; PostgreSQL is exposed on `localhost:5432` for local administration. The Compose volume preserves database contents across container restarts.

For Rust checks:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo check --workspace
```

## PostgreSQL deployment

Render provisions a persistent PostgreSQL database using the `basic-256mb` plan and injects its connection string as `DATABASE_URL`. The relay requires this setting and fails startup with a clear error if it is missing or cannot connect; it will not silently switch to ephemeral in-memory storage.

## Security status

NeonMonkey deliberately separates the following concerns:

1. MLS state and private keys remain on devices.
2. The relay stores opaque MLS protocol messages and delivery metadata only.
3. KeyPackage publication is required before another member can add a device.
4. Welcome and commit messages must be delivered losslessly and in order.
5. Server authentication must not be confused with MLS member authentication.
6. The Basic Credential implementation used by the initial crate is a starting point, not a production identity system.

A finished product requires audited identity-provider binding, secure key-at-rest storage, checkpoint storage, recovery, and interoperable multi-device support.

## License

MIT License. See [LICENSE](LICENSE).
