#!/bin/sh
# Builds the playground: the WebAssembly module into playground/pkg and the
# styles (written in stylet, of course) into playground/playground.css.
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli (same version as the crate).
set -e
cd "$(dirname "$0")/.."
cargo build --release -p stylet-wasm --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir playground/pkg \
  target/wasm32-unknown-unknown/release/stylet_wasm.wasm
cargo run --release -q -p stylet-cli -- build playground/playground.styl \
  -o playground/playground.css --minify --resolve-custom-media --source-map
echo "Built playground. Serve with: python3 -m http.server -d playground"
