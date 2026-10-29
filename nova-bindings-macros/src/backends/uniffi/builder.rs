use zyn::TokenStream;

use super::{FEATURE, UniFfiBackend};
use crate::backends::{BackendBuilder, ErrorGlue, doc_attrs};
use crate::codegen::delegate::concrete_indicator;
use crate::ir::{ErrorData, IndicatorData};
use crate::validation::diagnostics::with_help;

/// `#[derive(uniffi::Object)] pub struct UniffiCti { inner: Mutex<Cti<f64>> }`.
pub fn wrapper_struct(backend: &UniFfiBackend, data: &IndicatorData) -> TokenStream {
    let wrapper = backend.wrapper_ident(data);
    let public_name = &data.public_name;
    let inner = concrete_indicator(data, &backend.numeric_type());
    let vis = &data.vis;
    let docs = doc_attrs(&data.docs);
    template! {
        #[cfg(feature = {{ FEATURE }})]
        #[derive(::uniffi::Object)]
        #[uniffi(name = {{ public_name }})]
        {{ docs }}
        {{ vis }} struct {{ wrapper }} {
            pub(crate) inner: ::std::sync::Mutex<{{ inner }}>,
        }
    }
}

/// The error type itself becomes a flat UniFFI error: foreign code sees one
/// exception variant per Rust variant, carrying the `Display` message.
pub fn error_glue(error: &ErrorData) -> syn::Result<ErrorGlue> {
    if !error.is_enum {
        return Err(with_help(
            error.span,
            "UniFFI errors must be enums",
            "make the error an enum, or leave `uniffi` out of `#[bindings_error(backends(...))]`",
        ));
    }
    Ok(ErrorGlue {
        attrs: template! { #[cfg_attr(feature = {{ FEATURE }}, derive(::uniffi::Error), uniffi(flat_error))] },
        items: TokenStream::new(),
    })
}
