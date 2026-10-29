#!/usr/bin/env sh
# Builds the fixtures for wasm-bindgen and napi-rs and runs the Node tests.
# Requires wasm-pack, the wasm32-unknown-unknown target and Node.js >= 18.
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
target="$(cd "$here/../.." && pwd)/target"
out="$(mktemp -d)"

wasm-pack build "$here" --target nodejs --dev --out-dir "$out/wasm" --no-pack -- --features wasm
NOVA_WASM_PKG="$out/wasm" node --test "$here/js/wasm.test.cjs"

cargo build -p nova-bindings-integration --features napi
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) lib="$target/debug/nova_bindings_integration.dll" ;;
    Darwin) lib="$target/debug/libnova_bindings_integration.dylib" ;;
    *) lib="$target/debug/libnova_bindings_integration.so" ;;
esac
cp "$lib" "$out/fixtures.node"
NOVA_NAPI_ADDON="$out/fixtures.node" node --test "$here/js/napi.test.cjs"
