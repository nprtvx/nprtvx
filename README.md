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
4. Choose **Log in** later and enter the recovery phrase on the device where the identity was created.
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
