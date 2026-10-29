//! wasm-bindgen backend.
//!
//! `Cti<T>` becomes `WasmCti`, exported to JavaScript as class `Cti` with a
//! `new Cti(...)` constructor. Errors are thrown as JavaScript `Error`s whose
//! message is the Rust error's `Display` output.

mod builder;

use syn::{Ident, Path};
use zyn::{TokenStream, format_ident};

use crate::backends::types::{ScalarMapping, check_signature};
use crate::backends::{BackendBuilder, ErrorGlue, no_options};
use crate::ir::{
    BackendKind, BackendOption, BindingType, ErrorData, IndicatorData, MethodData, ScalarType,
};

pub const FEATURE: &str = "wasm";

pub struct WasmBackend;

pub struct WasmMapping;

impl ScalarMapping for WasmMapping {
    fn backend(&self) -> &'static str {
        "wasm"
    }

    fn foreign(&self, scalar: ScalarType) -> Result<ScalarType, &'static str> {
        Ok(scalar)
    }

    fn check_vec(&self, inner: &BindingType) -> Result<(), &'static str> {
        match inner {
            BindingType::Numeric | BindingType::String => Ok(()),
            BindingType::Scalar(ScalarType::Bool) => {
                Err("wasm-bindgen cannot pass `Vec<bool>`; use `Vec<u8>`")
            }
            BindingType::Scalar(_) => Ok(()),
            _ => Err("wasm-bindgen supports `Vec` of numbers or strings only"),
        }
    }
}

impl BackendBuilder for WasmBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Wasm
    }

    fn feature(&self) -> &'static str {
        FEATURE
    }

    fn wrapper_ident(&self, data: &IndicatorData) -> Ident {
        format_ident!("Wasm{}", data.rust_name)
    }

    fn validate_options(&self, options: &[BackendOption]) -> syn::Result<()> {
        no_options(self.kind(), options)
    }

    fn validate_method(&self, _data: &IndicatorData, method: &MethodData) -> syn::Result<()> {
        check_signature(&WasmMapping, &method.args, &method.output)
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
        Ok(builder::exported_impl(self, data, wrapper, methods))
    }

    fn validate_error_options(&self, options: &[BackendOption]) -> syn::Result<()> {
        no_options(self.kind(), options)
    }

    fn build_error(&self, error: &ErrorData) -> syn::Result<ErrorGlue> {
        Ok(crate::backends::display_check(FEATURE, &error.ident))
    }
}
