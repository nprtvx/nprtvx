FROM rust:1.94 AS builder
WORKDIR /app
COPY . .
# Cargo.lock needs to be regenerated for the current workspace package graph.
RUN cargo build --release -p airms-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/airms-server /usr/local/bin/airms-server
EXPOSE 8090
USER 65534:65534
CMD ["/usr/local/bin/airms-server"]
