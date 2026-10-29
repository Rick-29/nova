//! Compiler-checked completeness of the standard contract.
//!
//! No single macro invocation sees every impl block, so the derive cannot
//! tell whether `update` was ever bound. Instead it declares one hidden marker
//! trait per standard method and selected backend, and asserts that `()`
//! implements each of them. Every generated binding implements its marker:
//!
//! - a missing binding fails the assertion with the `on_unimplemented` note;
//! - a binding generated twice is a "conflicting implementations" error.
//!
//! Markers are not feature-gated, so these checks run in every Cargo
//! feature configuration, including one with no backend enabled.

use syn::Path;
use zyn::TokenStream;

use crate::codegen::naming::{marker_ident, sibling};
use crate::ir::{BackendKind, IndicatorData, MethodKind};

fn how_to_fix(data: &IndicatorData, kind: MethodKind, backend: BackendKind) -> String {
    let name = &data.rust_name;
    let method = kind.standard_name().expect("standard method");
    if data.config.is_overridden(kind, backend) {
        return format!(
            "`{name}` declares `overrides({method})` for `{}`: add an impl block marked `#[bindings(method = {method})]`",
            backend.name()
        );
    }
    match kind {
        MethodKind::Constructor => format!(
            "annotate the inherent impl block of `{name}` that defines `new` with `#[bindings]`"
        ),
        _ => format!("annotate the `Indicator` impl of `{name}` with `#[bindings]`"),
    }
}

/// Marker definitions and completeness assertions, emitted by the derive.
pub fn markers(data: &IndicatorData) -> TokenStream {
    let mut tokens = TokenStream::new();
    for backend in &data.config.backends {
        for kind in MethodKind::STANDARD {
            let ident = marker_ident(&data.rust_name, kind, *backend);
            let message = format!(
                "`{}` has no `{}` binding for backend `{}`",
                data.rust_name,
                kind.standard_name().expect("standard method"),
                backend.name()
            );
            let note = how_to_fix(data, kind, *backend);
            tokens.extend(template! {
                #[doc(hidden)]
                #[diagnostic::on_unimplemented(message = {{ message }}, label = "required by `#[derive(Bindings)]`", note = {{ note }})]
                pub(crate) trait {{ ident }} {}

                const _: fn() = || {
                    fn __nova_bindings_assert<T: {{ ident }}>() {}
                    __nova_bindings_assert::<()>();
                };
            });
        }
    }
    tokens
}

/// Implements the marker for one generated standard binding.
pub fn marker_impl(
    data: &IndicatorData,
    kind: MethodKind,
    backend: BackendKind,
    target: &Path,
) -> TokenStream {
    if kind == MethodKind::Custom {
        return TokenStream::new();
    }
    let marker = sibling(target, marker_ident(&data.rust_name, kind, backend));
    template! { impl {{ marker }} for () {} }
}
