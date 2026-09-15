# NeonMonkey

NeonMonkey is being rebuilt as a privacy-first messaging service. The release is being delivered in pieces so each security-sensitive layer can be reviewed before the final release.

## Run

Requires JDK 17 or newer and Maven.

```bash
mvn spring-boot:run
```

Open http://localhost:8080.

## Current piece: anonymous identities

The first piece removes email and phone signup:

- The browser generates an ECDH identity key pair.
- The server receives the public key and an account ID derived from it.
- The private key is encrypted in the browser with a recovery phrase before upload.
- The server stores only the public identity and encrypted recovery bundle.
- The recovery phrase is shown once and cannot be recovered by NeonMonkey.

The message transport is still the legacy plaintext placeholder at this stage. It must not be treated as end-to-end encrypted until the encrypted message transport piece is deployed.

## Release pieces

1. Anonymous identity and recovery phrase (current)
2. Encrypted one-to-one messaging
3. Encrypted group conversations
4. Disappearing messages
5. Encrypted attachments
6. Emoji and privacy-preserving GIF integrations
7. iOS and Android clients
8. Security review and final release

## Deploy on Render

This repository includes a `Dockerfile` and `render.yaml` for Render.

## Product flow

1. Open `neonmonkey.in`.
2. Choose **Create account**, enter a display name, and NeonMonkey generates the identity automatically.
3. Save the generated account ID and recovery phrase.
4. Choose **Restore account** later and enter the recovery phrase. New recovery phrases begin with the account ID so the encrypted bundle can be located on another device.
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

Do not set `PORT` manually. Render provides it automatically, and the container uses it to bind the Spring Boot server.

Optional GIF proxy configuration:

```text
GIF_PROVIDER_URL=<your provider search endpoint>
GIF_PROVIDER_KEY=<server-side provider key>
```

The browser never receives the provider key. Without these variables, emoji remains available and GIF search returns no results.

## Optional PostgreSQL persistence

The app uses in-memory storage when no database URL is configured, so local development
still starts with only `mvn spring-boot:run`. To persist identities, sessions, messages,
attachments, groups, group members, and group messages across restarts, set either
`JDBC_DATABASE_URL` or `DATABASE_URL` before starting the app. `DATABASE_URL` may use
Render's `postgres://...` format; a `jdbc:postgresql://...` URL is also accepted.

The normalized schema is in `db/schema.sql` and is applied automatically when a database
URL is present. For a Render Blueprint, attach a PostgreSQL database and expose its
connection string as `DATABASE_URL`.

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

NeonMonkey is available at http://localhost:8080. PostgreSQL data is stored in the
`neonmonkey-postgres` Docker volume and survives container restarts. To stop the
containers without deleting data:

```bash
docker compose down
```

Use `docker-compose down` when using the standalone binary.

Set `POSTGRES_DB`, `POSTGRES_USER`, `POSTGRES_PASSWORD`, or `APP_PORT` in a `.env`
file to override the development defaults. Do not use the default password in a
public deployment.

The application still needs an independent cryptographic audit before the custom browser
protocol should be considered production-grade. Native iOS and Android clients remain
separate client projects; the shared protocol and installable PWA are included here.
