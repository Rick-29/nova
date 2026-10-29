[![Discord](https://img.shields.io/badge/Discord-Join%20the%20community-5865F2?logo=discord&logoColor=white)](https://discord.gg/QwKmaaeW56)

# nova-bindings-macros

Procedural macros that generate foreign-language bindings for [Nova](https://crates.io/crates/nova-ta) technical-analysis indicators. You annotate an indicator once; the macros generate Python ([PyO3](https://pyo3.rs)), [UniFFI](https://mozilla.github.io/uniffi-rs/) (Kotlin, Swift, Python, ...), WebAssembly ([wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/)) and Node.js ([napi-rs](https://napi.rs)) wrappers, each behind its own Cargo feature.

The generated wrappers only delegate: they call the indicator's own constructor, `update`, `reset` and `ready`. No indicator logic is duplicated, so every language gets the same maths and warm-up semantics as Rust.

> **Status: early development.** Built for the `nova-ta` crate, where `Cti` and `UltimateSmoother` are the first annotated indicators. The attribute syntax may still change before 1.0.

## Design goals

- **One source of truth.** Bindings are derived from the annotated Rust signatures and doc comments.
- **Delegation only.** Wrappers convert types and errors, nothing else.
- **Compiler-checked contracts.** Missing, duplicated or mistyped bindings are compile errors with source spans.
- **No global state.** Each macro invocation stands alone; the struct's configuration reaches its impl blocks through generated code that rustc resolves.
- **Backends are independent.** Each backend consumes a shared, framework-free model of the indicator, so adding one does not touch the parser.

## Quick start

```rust
use nova_bindings_macros::{Bindings, bindings, bindings_error};
use nova_ta_core::{Indicator, Num};

/// Error returned by Nova indicators.
#[bindings_error]
#[derive(Debug, thiserror::Error)]
pub enum NovaError {
    #[error("Invalid period")]
    InvalidPeriod,
}

/// Correlation Trend Indicator (CTI).
#[derive(Bindings)]
#[bindings(error = NovaError)]
pub struct Cti<T: Num> {
    // fields
}

#[bindings]
impl<T: Num> Cti<T> {
    /// Creates a CTI indicator with the specified lookback period.
    pub fn new(period: usize) -> NovaResult<Self> {
        // existing constructor
    }
}

#[bindings]
impl<T: Num> Indicator for Cti<T> {
    type Input = T;
    type Output = T;
    type Error = NovaError;

    fn update(&mut self, input: Self::Input) -> NovaResult<Self::Output> { /* ... */ }
    fn reset(&mut self) { /* ... */ }
    fn ready(&self) -> bool { /* ... */ }
}
```

With the `python` feature this gives:

```python
from nova_ta import Cti, NovaError

cti = Cti(5)                      # NovaError (a ValueError) on an invalid period
value = cti.update(101.5)         # float
cti.ready()                       # bool
cti.reset()
```

The same indicator is available as `Cti` to UniFFI languages, `new Cti(5)` in JavaScript (wasm) and `new Cti(5)` in Node.js (napi).

Key points of the contract:

- Annotate the struct with `#[derive(Bindings)]` and `#[bindings(...)]`, and every impl block that provides bindable methods with `#[bindings]`.
- Impl blocks must come **after** the struct, in the same module. For an impl block elsewhere, name the struct: `#[bindings(target = path::to::Cti)]`.
- Generic type parameters are instantiated with `f64` by every backend (`Cti<T>` is exported as `Cti<f64>`). rustc checks that `f64` satisfies the bounds.
- `ready` is always the indicator's own `ready`; it is never inferred from a period.
- Every generated item is gated on its backend's Cargo feature, so a build with no binding feature compiles none of them.

## The binding contract

| method   | source                               | receiver    | arguments                              | returns                     |
| -------- | ------------------------------------ | ----------- | -------------------------------------- | --------------------------- |
| `new`    | inherent impl (`new`, or marked)     | none        | any supported, names preserved         | `Self` or `Result<Self, E>` |
| `update` | `Indicator` impl                     | `&mut self` | one scalar: a type parameter or `f64`  | the same, or `Result<_, E>` |
| `reset`  | `Indicator` impl                     | `&mut self` | none                                   | `()`                        |
| `ready`  | `Indicator` impl                     | `&self`     | none                                   | `bool`                      |
| extra    | inherent impl, `#[bindings(export)]` | `&self` / `&mut self` | any supported                | any supported               |

- `Result` aliases are recognised by name: any return type whose last segment ends in `Result` (`NovaResult<T>`) is treated as fallible, and rustc checks the rest.
- Errors are converted with `From<E> for <configured error>`, which rustc checks. An indicator whose `Error` is `Infallible` needs `impl From<Infallible> for MyError`.
- Supported argument and return types: `bool`, `f32`, `f64`, integer primitives, `String`, the indicator's type parameters, `Self::Input`-style associated types, and `Vec<_>` / `Option<_>` of those. References, tuples and other structs are rejected with a diagnostic. Backends add their own limits (see [Backends](#backends)).
- Doc comments on the struct and methods are copied to the generated classes and methods, and become Python docstrings, UniFFI docs and TypeScript comments.

### Inherent impl blocks

```rust
#[bindings]
impl<T: Num> Ema<T> {
    /// Uses `new` as the constructor by default.
    pub fn new(period: usize, smoothing: T) -> NovaResult<Self> { /* ... */ }

    /// A different function can be the constructor instead.
    // #[bindings(constructor)]
    // pub fn with_period(period: usize) -> Self { /* ... */ }

    /// Extra methods are opt-in, optionally renamed.
    #[bindings(export, name = "samples")]
    pub fn seen(&self) -> usize { self.seen }

    /// Not exported: no `#[bindings(...)]`.
    pub fn window(&self) -> &Window<T> { &self.window }
}
```

Method-level `#[bindings(...)]` attributes are removed from the emitted impl block, which is otherwise left unchanged.

## Configuration

`#[bindings(...)]` on the struct:

| argument | meaning |
| --- | --- |
| `error = Type` | **Required.** Error type exported to foreign languages. It must carry `#[bindings_error]`. |
| `backends(python, uniffi, wasm, napi)` | Backends to generate. Omitted: every implemented backend. |
| `name = "Name"` | Foreign class name. Default: the Rust struct name. |
| `overrides(update, new(python))` | Standard methods replaced by a custom adapter, for all selected backends or only the listed ones. See [Overrides](#overrides). |
| `python(module = "pkg", update = method \| call \| both)` | Python options: the `__module__` of the class, and whether `update` is exposed as `obj.update(x)` (default), `obj(x)`, or both. |

`uniffi(...)`, `wasm(...)` and `napi(...)` take no options yet. Unknown arguments, backends and options are compile errors that suggest the closest valid name:

```text
error: unknown backend `pyhton`

       help: did you mean `python`?
  --> src/statistics/cti.rs:47:21
   |
47 | #[bindings(backends(pyhton))]
   |                     ^^^^^^
```

## Overrides

An override replaces one standard binding with a custom adapter, for example to accept a batch in Python, without touching the indicator. It takes two declarations, because no single macro invocation sees every impl block:

```rust
#[derive(Bindings)]
#[bindings(error = NovaError, overrides(update(python)), python(update = both))]
pub struct Ema<T> { /* ... */ }

// ... the usual inherent and `Indicator` impl blocks ...

/// Python adapter: `update` takes a list and returns one output per value.
#[bindings(method = update)]
impl Ema<f64> {
    /// Feeds a batch of values and returns every output.
    pub fn update_batch(&mut self, values: Vec<f64>) -> NovaResult<Vec<f64>> {
        values.into_iter().map(|value| self.update(value)).collect()
    }
}
```

- `overrides(update(python))` on the struct stops the default Python `update` from being generated. Other backends keep the default.
- `#[bindings(method = update)]` marks the block that provides the replacement. It must contain exactly one function, whose receiver matches the standard method's. Its arguments and return type may be any supported types. `backends(...)` on the block narrows it further.
- The adapter is an ordinary inherent method, still callable from Rust. It must not be named `update`, `reset` or `ready`, or it would shadow the trait method for Rust callers.
- The Python `update` policy applies to the override too: with `update = both`, `ema([1.0, 2.0])` works.
- An override that is declared but never provided, or provided twice, is a compile error.

## The error type

`#[bindings_error]` on the error enum generates the conversions every wrapper relies on, once per crate:

| backend | generated |
| --- | --- |
| Python | An exception class with the enum's name (subclass of `ValueError`), exposed in Rust as `Py<Name>`, plus `impl From<Name> for PyErr`. Options: `python(module = "pkg", base = RuntimeError)`. `base` is one of `Exception`, `ValueError`, `RuntimeError`, `ArithmeticError`, `TypeError`, `IndexError`. |
| UniFFI | `#[derive(uniffi::Error)]` as a flat error: one foreign exception per variant, carrying the `Display` message. Requires an enum. |
| wasm, napi | Errors are thrown as JavaScript `Error`s with the `Display` message. A compile-time check ensures `Display` is implemented. |

`backends(...)` restricts which glue is generated, for example `#[bindings_error(backends(python))]` for a struct error type.

## Generated items

For `Cti<T>`, next to the struct:

| backend | Rust wrapper | foreign name | holds |
| --- | --- | --- | --- |
| Python | `PyCti` | `#[pyclass(name = "Cti")]` | `Cti<f64>` |
| UniFFI | `UniffiCti` | `uniffi::Object` named `Cti` | `Mutex<Cti<f64>>` |
| wasm | `WasmCti` | `js_name = Cti` | `Cti<f64>` |
| napi | `NapiCti` | `js_name = "Cti"` | `Cti<f64>` |

Each `#[bindings]` impl block becomes one `#[pymethods]`, `#[uniffi::export]`, `#[wasm_bindgen]` or `#[napi]` block on the wrapper.

## Cargo features and dependencies

The macro crate itself only depends on `syn` and `zyn`. The crate that uses the macros owns the backend dependencies and must declare these exact feature names, since the generated code is gated on `#[cfg(feature = "...")]` evaluated in that crate:

```toml
[features]
python = ["dep:pyo3"]
python-stubs = ["python", "dep:pyo3-stub-gen"]
uniffi = ["dep:uniffi"]
wasm = ["dep:wasm-bindgen"]
napi = ["dep:napi", "dep:napi-derive"]

[dependencies]
nova-bindings-macros = "0.1"
pyo3 = { version = "0.30", optional = true, features = ["multiple-pymethods"] }
pyo3-stub-gen = { version = "0.23", optional = true }
uniffi = { version = "0.32", optional = true }
wasm-bindgen = { version = "0.2", optional = true }
napi = { version = "3", optional = true, default-features = false, features = ["napi4"] }
napi-derive = { version = "3", optional = true }
```

- Declare a feature for every backend you select, or rustc warns about an unexpected `cfg`. Use `backends(...)` to select fewer.
- Selecting a backend in the macro controls **what code is generated**; enabling its feature controls **whether it is compiled**. A backend that is selected but not enabled costs nothing.
- Generated code refers to `::pyo3`, `::pyo3_stub_gen`, `::uniffi`, `::wasm_bindgen`, `::napi` and `::napi_derive`, so do not rename these dependencies.
- Tested with pyo3 0.30.1, pyo3-stub-gen 0.23.2, uniffi 0.32.2, wasm-bindgen 0.2.129, napi 3.15 and napi-derive 3.6.

## Backends

### Python (PyO3)

- Every `#[bindings]` impl block is its own `#[pymethods]` block, so PyO3's `multiple-pymethods` feature is required.
- Register the classes and the exception in your module:

  ```rust
  #[pyo3::pymodule]
  fn nova_ta(m: &Bound<'_, PyModule>) -> PyResult<()> {
      m.add_class::<PyCti>()?;
      m.add("NovaError", m.py().get_type::<PyNovaError>())?;
      Ok(())
  }
  ```

- Constructors keep their parameter names (`Cti(period=5)`), and `inspect.signature` reports them.
- **Stubs.** With the `python-stubs` feature every generated item is also registered with [pyo3-stub-gen](https://crates.io/crates/pyo3-stub-gen). Follow its workflow: `pyo3_stub_gen::define_stub_info_gatherer!(stub_info);` in the library, a small binary that calls `stub_info()?.generate()`, and a `pyproject.toml` naming the module. In `nova-ta` (a mixed maturin layout, `python-source = "python"`), `cargo run --bin stub_gen --features python-stubs` writes `python/nova_ta/__init__.pyi`, which maturin packs into the wheel.

### UniFFI

- Add `uniffi::setup_scaffolding!();` to the crate root, build a `cdylib`, and generate bindings with UniFFI's own `uniffi-bindgen generate --library ...`.
- Objects are shared behind `Arc` and called through `&self`, so the indicator sits in a `Mutex`. Concurrent calls from several foreign threads are serialized; each `update` sees the state left by the previous one. If a call panics while holding the lock, the state is unspecified, as after an `update` error: call `reset` before reuse.
- UniFFI has no `usize`/`isize`; they cross as `u64`/`i64`. An argument that does not fit the target's pointer width panics, which UniFFI reports as an internal error.

### WebAssembly (wasm-bindgen)

- Classes are exported with `js_name`; constructors are `new Cti(...)`. Errors are thrown as `Error` with the Rust message.
- `Vec` arguments must hold numbers or strings (`Vec<bool>` is not supported by wasm-bindgen).

### Node.js (napi-rs)

- napi-rs camel-cases method names (`update_batch` → `updateBatch`).
- N-API has no `usize`: it is accepted as `u32` and returned as a JavaScript number. `u64`, `i8`, `i16`, `u8`, `u16` and `f32` are rejected with a diagnostic.

## Cross-compiling

`nova-bindings-macros` is a procedural macro, so it always runs on the build host, and its output is ordinary Rust that compiles for whatever target you pick. Each toolchain handles its own target:

- Python wheels: `maturin build --release --target <triple>` (for example with `--zig` for manylinux).
- UniFFI: build the `cdylib` for each target (`cargo ndk` for Android, `cargo build --target aarch64-apple-ios` for iOS), then run `uniffi-bindgen` on one of the libraries.
- wasm: `wasm-pack build --target web -- --features wasm`, or `cargo build --target wasm32-unknown-unknown` followed by `wasm-bindgen`.
- napi: `napi build --platform --release --target <triple> --features napi`.

## Architecture

```
src/
  lib.rs           entry points: #[derive(Bindings)], #[bindings], #[bindings_error]
  macros/          one expansion per macro, written against zyn::TokenStream
  parser/          syntax -> IR: struct, impl blocks, signatures, attributes, docs, overrides
  ir/              project-owned model: indicator, methods, types, config, docs
  validation/      the standard contract, supported types, span-accurate diagnostics
  codegen/         backend-neutral: delegation calls, markers, configuration carrier
  backends/        one builder per framework, registered in factory.rs
```

- **Parse once, normalize, validate, generate.** Backends receive the IR and never re-read source or attribute syntax.
- **Configuration carrier.** The derive emits a hidden `macro_rules!` holding the struct declaration. Each `#[bindings]` impl block expands to a call of that macro, which passes the struct and the block to a hidden expansion macro. This is why impl blocks must follow the struct, and why `target = ...` reaches impl blocks in other modules.
- **Completeness markers.** For every selected backend and standard method the derive declares a hidden marker trait and asserts it is implemented; every generated binding implements its marker. A missing `update` fails with "`Cti` has no `update` binding for backend `python`", and an `update` generated twice is a conflicting-implementation error. These checks are not feature-gated, so they run in every configuration.
- **Adding a backend** means a `BackendKind` variant, a builder implementing `BackendBuilder`, one line in `backends/factory.rs`, a feature name and tests. The parser and IR stay unchanged.

Code generation uses [zyn](https://crates.io/crates/zyn) templates and argument parsing. Diagnostics are `syn::Error`s, so they keep exact spans on stable Rust.

## Development

```sh
cargo test -p nova-bindings-macros                  # unit tests + compile-fail UI tests
TRYBUILD=overwrite cargo test --test ui             # refresh tests/ui/fail/*.stderr after changing a message
cargo test -p nova-bindings-integration --features python,uniffi
sh integration/scripts/js-tests.sh                  # wasm (wasm-pack) and napi, run with Node.js
```

`integration/` is an unpublished crate with fixture indicators covering overrides, the Python callable policy, exported methods, generic constructor arguments, an explicit constructor and impl blocks in another module. In `nova-ta`, `python/tests` checks the built extension and `scripts/uniffi-python-test.sh` checks the UniFFI-generated Python module.

## Help and community

Questions or feedback? Join the [Nova Discord](https://discord.gg/QwKmaaeW56).

## License

Licensed under the [MIT license](LICENSE).
