# Operations

## Health checks

- `/health` reports process and configured dependency status.
- `/ready` performs a PostgreSQL connectivity check when `DATABASE_URL` is
  configured and should be used for readiness routing.

## Required production settings

- Set `DATABASE_URL` to a private PostgreSQL connection string.
- Set `NEONMONKEY_COOKIE_SECURE=true` behind HTTPS.
- Do not use the Docker Compose development password.
- Keep `GIF_PROVIDER_KEY` server-side; never expose it to the browser.
- Terminate TLS at the platform ingress or a trusted reverse proxy.

## PostgreSQL backup and restore

Use the platform's encrypted, automated PostgreSQL backups where available.
Before a release involving schema changes, take a backup and verify that a
restore can start the application and pass `/ready`.

Example manual commands:

```bash
pg_dump --format=custom "$DATABASE_URL" > neonmonkey-$(date +%Y%m%d-%H%M%S).dump
createdb "$RESTORE_DATABASE_URL"
pg_restore --exit-on-error --dbname="$RESTORE_DATABASE_URL" neonmonkey.dump
```

Never place connection strings or backup files in the repository.

## Incident response

1. Remove or rotate compromised database, provider, and deployment credentials.
2. Disable public traffic at the ingress if message confidentiality or account
   integrity is uncertain.
3. Preserve application and database audit logs without copying plaintext
   messages or private key material.
4. Restore into an isolated database and validate `/ready`, authentication,
   expiry cleanup, and migration state.
5. Require security review before restoring production traffic.

The current service is not a complete end-to-end encrypted messenger. Treat
the browser message payload as sensitive application data until the reviewed
client cryptographic protocol is implemented.
