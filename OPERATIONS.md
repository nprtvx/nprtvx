# NeonMonkey Operations

## Local relay

```bash
cargo run -p airms-server
curl http://localhost:8090/health
```

## PostgreSQL relay

```bash
docker compose up --build
```

PostgreSQL is used for opaque MLS relay records. Application content and MLS private state must never be stored as plaintext on the relay.
