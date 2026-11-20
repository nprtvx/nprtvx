# NeonMonkey Operations

## Local PostgreSQL relay

```bash
docker compose up --build
curl http://localhost:8090/health
```

The Compose configuration starts PostgreSQL with a persistent named volume and waits for its health check before starting the relay. The database is exposed on port 5432 for local administration.

## Production PostgreSQL relay

The Render blueprint provisions a persistent PostgreSQL instance and injects its connection string into `DATABASE_URL`. The server does not fall back to in-memory storage if PostgreSQL is missing or unavailable; startup fails with the connection error so the deployment cannot appear healthy while losing relay data.

PostgreSQL stores opaque MLS relay records only. Application content and MLS private state must never be stored as plaintext on the relay.
