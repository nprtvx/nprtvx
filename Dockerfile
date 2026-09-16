FROM rust:1.98-bookworm AS build

WORKDIR /src
COPY Cargo.toml Cargo.lock ./

RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version 0.2.128 --locked
COPY crates ./crates
COPY db ./db

RUN cargo build --release -p neonmonkey-server \
    && crates/web/build.sh /src/static

FROM debian:bookworm-slim

WORKDIR /app
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates wget \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /src/target/release/neonmonkey-server ./neonmonkey-server
COPY --from=build /src/static ./static

RUN useradd --system --home-dir /app --no-create-home neonmonkey \
    && chown -R neonmonkey:neonmonkey /app
USER neonmonkey

ENV STATIC_DIR=/app/static
EXPOSE 10000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["wget", "--spider", "--quiet", "http://127.0.0.1:10000/health"]

CMD ["./neonmonkey-server"]
