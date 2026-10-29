# Compiling the bindings

Every language binding is a Cargo feature of `nova-ta`, and every build below writes its output to `bindings/<language>/`, which is git-ignored. Delete the folder at any time; nothing in it is source.

| Language | Feature | Tool | Output |
| --- | --- | --- | --- |
| Python | `python` + `python-stubs` | maturin, pyo3-stub-gen | `bindings/python/nova_ta-*.whl` |
| Kotlin / Android | `uniffi` | uniffi-bindgen | `bindings/kotlin/` |
| Swift / iOS / macOS | `uniffi` | uniffi-bindgen | `bindings/swift/` |
| Ruby | `uniffi` | uniffi-bindgen | `bindings/ruby/` |
| Python (UniFFI) | `uniffi` | uniffi-bindgen | `bindings/python-uniffi/` |
| JavaScript (WebAssembly) | `wasm` | wasm-bindgen | `bindings/wasm/` |
| Node.js (native) | `napi` | cargo | `bindings/node/nova_ta.node` |

Commands are run from the repository root. The `python` features are on by default, so every non-Python build passes `--no-default-features`.

The native library Cargo produces has a platform-specific name. Set `LIB` once for your machine:

```sh
LIB=target/release/libnova_ta.so      # Linux
LIB=target/release/libnova_ta.dylib   # macOS
LIB=target/release/nova_ta.dll        # Windows
```

## Python

Requires [maturin](https://www.maturin.rs) (`pip install maturin`).

```sh
cargo run --release --bin stub_gen --features python-stubs   # writes python/nova_ta/__init__.pyi
maturin build --release --out bindings/python
```

The wheel contains the extension, the `.pyi` stubs and a `py.typed` marker:

```sh
pip install bindings/python/nova_ta-*.whl
python -c "from nova_ta import Cti; print(Cti(5).update(1.0))"
```

Other targets: `maturin build --release --target aarch64-unknown-linux-gnu --zig --out bindings/python`. Each wheel is built for one Python version; pass `--interpreter python3.12` to choose it.

## UniFFI: Kotlin, Swift, Ruby

Build the native library once, then generate the code for each language from it:

```sh
cargo rustc --release --lib --no-default-features --features uniffi --crate-type cdylib

bindgen() {   # bindgen <language> [folder]
    out="bindings/${2:-$1}"
    mkdir -p "$out" && cp "$LIB" "$out/"
    cargo run --release --no-default-features --features uniffi-cli --bin uniffi-bindgen -- \
        generate --library "$out/$(basename "$LIB")" --language "$1" --out-dir "$out"
}
bindgen kotlin
bindgen swift
bindgen ruby
```

Each folder now holds the generated source next to a copy of the native library. Warnings about `ktlint`, `swift-format` or `rubocop` only mean the generated code was not auto-formatted.

### Kotlin

`bindings/kotlin/uniffi/nova_ta/nova_ta.kt` (package `uniffi.nova_ta`) loads the library through [JNA](https://github.com/java-native-access/jna):

```kotlin
dependencies { implementation("net.java.dev.jna:jna:5.17.0@aar") }   // drop `@aar` on the JVM
```

- **JVM:** put the library on `-Djna.library.path=bindings/kotlin`, or in `src/main/resources/<os>-<arch>/` (for example `linux-x86-64/libnova_ta.so`).
- **Android:** build one `.so` per ABI with [cargo-ndk](https://github.com/bbqsrc/cargo-ndk) and copy it to `jniLibs`:

  ```sh
  cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 rustc --release --lib --no-default-features --features uniffi --crate-type cdylib
  # target/<triple>/release/libnova_ta.so -> app/src/main/jniLibs/<abi>/libnova_ta.so
  ```

```kotlin
import uniffi.nova_ta.Cti

Cti(5uL).use { cti -> println(cti.update(1.0)) }   // `usize` arguments are `ULong`
```

### Swift

`bindings/swift/` holds `nova_ta.swift`, `nova_taFFI.h` and `nova_taFFI.modulemap`. The Swift code is the same for every Apple platform; only the library changes. On macOS, build static libraries and wrap them in an XCFramework:

```sh
for target in aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim; do
    rustup target add "$target"
    cargo rustc --release --lib --no-default-features --features uniffi --crate-type staticlib --target "$target"
done

mkdir -p bindings/swift/include
cp bindings/swift/nova_taFFI.h bindings/swift/include/
cp bindings/swift/nova_taFFI.modulemap bindings/swift/include/module.modulemap
xcodebuild -create-xcframework \
    -library target/aarch64-apple-darwin/release/libnova_ta.a -headers bindings/swift/include \
    -library target/aarch64-apple-ios/release/libnova_ta.a -headers bindings/swift/include \
    -library target/aarch64-apple-ios-sim/release/libnova_ta.a -headers bindings/swift/include \
    -output bindings/swift/NovaTaFFI.xcframework
```

Add `NovaTaFFI.xcframework` and `nova_ta.swift` to the Xcode project or Swift package.

### Ruby

Requires the [ffi](https://github.com/ffi/ffi) gem (`gem install ffi`). The generated `nova_ta.rb` loads `nova_ta` from the library search path:

```sh
LD_LIBRARY_PATH=bindings/ruby ruby -I bindings/ruby -e 'require "nova_ta"; puts NovaTa::Cti.new(5).update(1.0)'
```

On macOS use `DYLD_LIBRARY_PATH`; on Windows put `bindings/ruby` on `PATH`.

### Python through UniFFI (optional)

The PyO3 wheel above is the recommended Python package. UniFFI can also generate a pure-Python `ctypes` module, which is handy for checking the UniFFI output:

```sh
bindgen python python-uniffi   # kept apart from the wheel in bindings/python
python -c "import sys; sys.path.insert(0, 'bindings/python-uniffi'); import nova_ta; print(nova_ta.Cti(5).update(1.0))"
```

C#, Go and other languages are supported by third-party generators such as [uniffi-bindgen-cs](https://github.com/NordSecurity/uniffi-bindgen-cs) and [uniffi-bindgen-go](https://github.com/NordSecurity/uniffi-bindgen-go). They take the same `--library` argument.

## WebAssembly

Requires the wasm target and a `wasm-bindgen` CLI whose version **exactly** matches the `wasm-bindgen` crate in `Cargo.lock`:

```sh
rustup target add wasm32-unknown-unknown
cargo pkgid wasm-bindgen                              # ...#wasm-bindgen@0.2.129
cargo install wasm-bindgen-cli --version 0.2.129

cargo rustc --release --lib --no-default-features --features wasm --crate-type cdylib --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir bindings/wasm target/wasm32-unknown-unknown/release/nova_ta.wasm
```

`bindings/wasm/` contains `nova_ta.js`, `nova_ta_bg.wasm` and TypeScript definitions with the Rust docs. For Node.js or a bundler, use `--target nodejs` or `--target bundler`.

```js
import init, { Cti } from "./bindings/wasm/nova_ta.js";

await init();
const cti = new Cti(5);
console.log(cti.update(1.0));
```

## Node.js (native addon)

```sh
cargo rustc --release --lib --no-default-features --features napi --crate-type cdylib
mkdir -p bindings/node && cp "$LIB" bindings/node/nova_ta.node
```

On macOS, the addon must leave the Node.js symbols to be resolved at load time:

```sh
cargo rustc --release --lib --no-default-features --features napi --crate-type cdylib \
    -- -C link-arg=-undefined -C link-arg=dynamic_lookup
```

```js
const { Cti } = require("./bindings/node/nova_ta.node");
console.log(new Cti(5).update(1.0));
```

The addon is built for the current platform and Node-API 4+. For other platforms add `--target <triple>`. For generated TypeScript definitions and npm packaging, use the [napi-rs CLI](https://napi.rs/docs/cli/build) (`napi build --platform --release --features napi`).

## Notes

- Each `cargo rustc` above overwrites the same file in `target/release/`, so copy the library to `bindings/` before building for the next language.
- `--crate-type cdylib` (or `staticlib`) is passed on the command line, so `nova-ta` stays a plain `rlib` for Rust users.
- Generic indicators are exported with `f64`. Errors keep the `NovaError` messages: a `NovaError` exception in Python, `NovaException` in Kotlin, `NovaError` in Swift and Ruby, and an `Error` in JavaScript.
