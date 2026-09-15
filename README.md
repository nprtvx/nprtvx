# Gather Chat

A small Spring Boot web chat app for a polished static team chat interface. Messages are stored in memory for the life of the server.

## Run

Requires JDK 17 or newer and Maven.

```bash
mvn spring-boot:run
```

Open http://localhost:8080.

## Deploy on Render

This repository includes a `Dockerfile` and `render.yaml` for Render.

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
