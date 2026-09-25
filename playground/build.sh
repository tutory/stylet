#!/bin/sh
# Builds the WebAssembly module into playground/pkg.
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli (same version as the crate).
set -e
cd "$(dirname "$0")/.."
cargo build --release -p stylet-wasm --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir playground/pkg \
  target/wasm32-unknown-unknown/release/stylet_wasm.wasm
echo "Built playground/pkg. Serve with: python3 -m http.server -d playground"
