# NeonMonkey

NeonMonkey is a Rust-first private messenger foundation built around **OpenMLS**.

> This repository is an implementation in progress. Cryptography is provided by OpenMLS, not by a custom “envelope” implementation.

## Run locally

Start PostgreSQL and the relay together:

```bash
docker compose up --build
```

Or run the Rust checks directly:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo check --workspace
```

The relay package is `neonmonkey-server`:

```bash
cargo run -p neonmonkey-server
```

## PostgreSQL

Set `DATABASE_URL` to a PostgreSQL connection string. NeonMonkey requires PostgreSQL and will not silently fall back to in-memory storage.

```bash
DATABASE_URL=postgres://neonmonkey:neonmonkey@localhost:5432/neonmonkey cargo run -p neonmonkey-server
```

## License

MIT License. See [LICENSE](LICENSE).
