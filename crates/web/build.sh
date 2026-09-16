#!/usr/bin/env sh
set -eu

# Install wasm-bindgen-cli once (`cargo install wasm-bindgen-cli`), then run
# this script from the repository root. The output directory can be pointed at
# Axum's STATIC_DIR or copied into its static directory.
OUT="${1:-target/neonmonkey-web}"
rm -rf "$OUT"
mkdir -p "$OUT"
cargo build -p neonmonkey-web --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/neonmonkey_web.wasm \
  --target web --out-dir "$OUT"
cp crates/web/static/index.html crates/web/static/style.css "$OUT/"
