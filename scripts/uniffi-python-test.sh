#!/usr/bin/env sh
# Builds nova-ta with the `uniffi` feature, generates Python bindings with
# UniFFI's own uniffi-bindgen and runs python/uniffi/test_uniffi_bindings.py.
set -eu
cd "$(dirname "$0")/.."

cargo rustc -p nova-ta --lib --no-default-features --features uniffi --crate-type cdylib
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) lib=target/debug/nova_ta.dll ;;
    Darwin) lib=target/debug/libnova_ta.dylib ;;
    *) lib=target/debug/libnova_ta.so ;;
esac

out="$(mktemp -d)"
cargo run -p nova-ta --no-default-features --features uniffi-cli --bin uniffi-bindgen -- \
    generate --library "$lib" --language python --out-dir "$out"
cp "$lib" "$out/"
cp python/uniffi/test_uniffi_bindings.py "$out/"
cd "$out"
python -m unittest -v test_uniffi_bindings
