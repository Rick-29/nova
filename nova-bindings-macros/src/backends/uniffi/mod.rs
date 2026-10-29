//! UniFFI backend.
//!
//! `Cti<T>` becomes `UniffiCti`, a `uniffi::Object` exported as `Cti`.
//! UniFFI objects are shared behind `Arc` and called through `&self`, so the
//! indicator lives in a `Mutex`: concurrent calls from several foreign
//! threads are serialized, and each `update` sees the state left by the
//! previous one. A panic while the lock is held leaves the state unspecified,
//! exactly as an `update` error does; call `reset` before reusing it.
//!
//! UniFFI has no `usize`/`isize`, so they cross as `u64`/`i64`.

mod builder;
mod methods;

use syn::{Ident, Path};
use zyn::{TokenStream, format_ident};

use crate::backends::types::{ScalarMapping, check_signature};
use crate::backends::{BackendBuilder, ErrorGlue, no_options};
use crate::ir::{BackendKind, BackendOption, ErrorData, IndicatorData, MethodData, ScalarType};

pub const FEATURE: &str = "uniffi";

pub struct UniFfiBackend;

pub struct UniFfiMapping;

impl ScalarMapping for UniFfiMapping {
    fn backend(&self) -> &'static str {
        "uniffi"
    }

    fn foreign(&self, scalar: ScalarType) -> Result<ScalarType, &'static str> {
        Ok(match scalar {
            ScalarType::Usize => ScalarType::U64,
            ScalarType::Isize => ScalarType::I64,
            other => other,
        })
    }

    fn lift(&self, scalar: ScalarType, value: TokenStream) -> Option<TokenStream> {
        let (target, source) = match scalar {
            ScalarType::Usize => (template! { usize }, template! { u64 }),
            ScalarType::Isize => (template! { isize }, template! { i64 }),
            _ => return None,
        };
        Some(template! {
            <{{ target }} as ::core::convert::TryFrom<{{ source }}>>::try_from({{ value }})
                .expect("integer argument does not fit in this platform's pointer width")
        })
    }

    fn lower(&self, scalar: ScalarType, value: TokenStream) -> Option<TokenStream> {
        match scalar {
            ScalarType::Usize => Some(template! { ({{ value }}) as u64 }),
            ScalarType::Isize => Some(template! { ({{ value }}) as i64 }),
            _ => None,
        }
    }
}

impl BackendBuilder for UniFfiBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::UniFfi
    }

    fn feature(&self) -> &'static str {
        FEATURE
    }

    fn wrapper_ident(&self, data: &IndicatorData) -> Ident {
        format_ident!("Uniffi{}", data.rust_name)
    }

    fn validate_options(&self, options: &[BackendOption]) -> syn::Result<()> {
        no_options(self.kind(), options)
    }

    fn validate_method(&self, _data: &IndicatorData, method: &MethodData) -> syn::Result<()> {
        check_signature(&UniFfiMapping, &method.args, &method.output)
    }

    fn build_struct(&self, data: &IndicatorData) -> syn::Result<TokenStream> {
        Ok(builder::wrapper_struct(self, data))
    }

    fn build_methods(
        &self,
        data: &IndicatorData,
        wrapper: &Path,
        methods: &[&MethodData],
    ) -> syn::Result<TokenStream> {
        Ok(methods::exported_impl(self, data, wrapper, methods))
    }

    fn validate_error_options(&self, options: &[BackendOption]) -> syn::Result<()> {
        no_options(self.kind(), options)
    }

    fn build_error(&self, error: &ErrorData) -> syn::Result<ErrorGlue> {
        builder::error_glue(error)
    }
}
