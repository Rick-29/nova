//! Procedural macros that generate foreign-language bindings for
//! [Nova](https://crates.io/crates/nova-ta) technical-analysis indicators.
//!
//! Supported backends, each behind a Cargo feature of the crate that uses the
//! macros:
//!
//! | backend  | framework    | feature  |
//! |----------|--------------|----------|
//! | `python` | PyO3         | `python` (+ `python-stubs` for `pyo3-stub-gen`) |
//! | `uniffi` | UniFFI       | `uniffi` |
//! | `wasm`   | wasm-bindgen | `wasm`   |
//! | `napi`   | napi-rs      | `napi`   |
//!
//! ```ignore
//! #[derive(Bindings)]
//! #[bindings(error = NovaError)]
//! pub struct Cti<T: Num> { /* ... */ }
//!
//! #[bindings]
//! impl<T: Num> Cti<T> {
//!     pub fn new(period: usize) -> NovaResult<Self> { /* ... */ }
//! }
//!
//! #[bindings]
//! impl<T: Num> Indicator for Cti<T> { /* update, reset, ready */ }
//! ```
//!
//! The generated wrappers delegate to the original constructor and methods;
//! no indicator logic is duplicated. See the README for the full contract,
//! configuration options and the required dependencies.

/// `zyn!` template evaluated to a `TokenStream`.
macro_rules! template {
    ($($tokens:tt)*) => {{
        let input = ::zyn::Input::default();
        let _ = &input;
        ::zyn::TokenStream::from(::zyn::zyn!($($tokens)*))
    }};
}

mod backends;
mod codegen;
mod ir;
mod macros;
mod parser;
mod validation;

use proc_macro::TokenStream;

/// Generates the backend wrapper types for an indicator struct.
///
/// Configuration goes in `#[bindings(...)]` on the struct:
///
/// - `error = Type` (required): error type exported to foreign languages.
///   It must be annotated with [`macro@bindings_error`].
/// - `backends(python, uniffi, wasm, napi)`: backends to generate; all by default.
/// - `name = "Name"`: foreign class name; the Rust name by default.
/// - `overrides(update, new(python))`: standard methods replaced by a custom
///   adapter (`#[bindings(method = ...)]`), for all or some backends.
/// - `python(module = "pkg", update = method | call | both)`: Python options.
///
/// Every impl block providing bindable methods must be annotated with
/// [`macro@bindings`] and appear after the struct in the same module (or use
/// `#[bindings(target = path::Indicator)]`).
#[proc_macro_derive(Bindings, attributes(bindings))]
pub fn derive_bindings(input: TokenStream) -> TokenStream {
    macros::derive_bindings::expand(input.into()).into()
}

/// Generates bindings for the methods of one impl block of an indicator.
///
/// - On an inherent impl block: `new` (or the function marked
///   `#[bindings(constructor)]`) becomes the constructor, and functions
///   marked `#[bindings(export)]` (optionally `name = "..."`) become extra methods.
/// - On a trait impl block (`impl Indicator for ...`): `update`, `reset` and
///   `ready` become the standard methods.
/// - `#[bindings(method = update)]` (optionally `backends(...)`): the block's
///   single function replaces the standard `update` binding. The struct must
///   declare `overrides(update)`.
/// - `target = path::Indicator`: names the indicator when it cannot be
///   inferred from the self type or lives in another module.
///
/// The impl block itself is emitted unchanged.
#[proc_macro_attribute]
pub fn bindings(args: TokenStream, item: TokenStream) -> TokenStream {
    macros::bindings_impl::expand_attribute(args.into(), item.into()).into()
}

/// Generates the foreign error conversions for an error type.
///
/// - Python: an exception class (subclass of `ValueError` by default) exposed
///   as `Py<Name>`, and `From<Name> for PyErr`.
///   Options: `python(module = "pkg", base = RuntimeError)`.
/// - UniFFI: derives `uniffi::Error` as a flat error (enums only).
/// - wasm / napi: errors are thrown with their `Display` message.
///
/// `backends(...)` restricts the generated glue.
#[proc_macro_attribute]
pub fn bindings_error(args: TokenStream, item: TokenStream) -> TokenStream {
    macros::bindings_error::expand(args.into(), item.into()).into()
}

#[doc(hidden)]
#[proc_macro]
pub fn __bindings_impl(input: TokenStream) -> TokenStream {
    macros::bindings_impl::expand_carried(input.into()).into()
}
