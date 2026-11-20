# NeonMonkey Operations

## Local relay

```bash
docker compose up --build
curl http://localhost:8090/health
```

The relay package is `neonmonkey-server`. PostgreSQL is persistent through the `neonmonkey-postgres` named volume.

## PostgreSQL relay

Set `DATABASE_URL` to the deployed PostgreSQL connection string. Startup fails if PostgreSQL is missing or unavailable.
