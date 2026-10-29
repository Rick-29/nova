//! napi-rs backend.
//!
//! `Cti<T>` becomes `NapiCti`, exported to Node.js as class `Cti`. napi-rs
//! camel-cases method names (`update_batch` → `updateBatch`). Errors are
//! thrown as JavaScript `Error`s carrying the Rust `Display` message.
//!
//! N-API has no `usize`: it is accepted as `u32` and returned as a
//! JavaScript number (`f64`, exact below 2^53).

mod builder;

use syn::{Ident, Path};
use zyn::{TokenStream, format_ident};

use crate::backends::types::{ScalarMapping, check_signature};
use crate::backends::{BackendBuilder, ErrorGlue, no_options};
use crate::ir::{BackendKind, BackendOption, ErrorData, IndicatorData, MethodData, ScalarType};

pub const FEATURE: &str = "napi";

pub struct NapiBackend;

pub struct NapiMapping;

impl ScalarMapping for NapiMapping {
    fn backend(&self) -> &'static str {
        "napi"
    }

    fn foreign(&self, scalar: ScalarType) -> Result<ScalarType, &'static str> {
        match scalar {
            ScalarType::Bool
            | ScalarType::F64
            | ScalarType::I32
            | ScalarType::U32
            | ScalarType::I64 => Ok(scalar),
            ScalarType::Usize => Ok(ScalarType::U32),
            ScalarType::Isize => Ok(ScalarType::I32),
            _ => Err("N-API numbers support bool, f64, i32, u32, i64, usize and isize"),
        }
    }

    fn foreign_out(&self, scalar: ScalarType) -> Result<ScalarType, &'static str> {
        match scalar {
            ScalarType::Usize | ScalarType::Isize => Ok(ScalarType::F64),
            other => self.foreign(other),
        }
    }

    fn lift(&self, scalar: ScalarType, value: TokenStream) -> Option<TokenStream> {
        // Lossless: Node.js only runs on 32- and 64-bit targets.
        match scalar {
            ScalarType::Usize => Some(template! { ({{ value }}) as usize }),
            ScalarType::Isize => Some(template! { ({{ value }}) as isize }),
            _ => None,
        }
    }

    fn lower(&self, scalar: ScalarType, value: TokenStream) -> Option<TokenStream> {
        match scalar {
            ScalarType::Usize | ScalarType::Isize => Some(template! { ({{ value }}) as f64 }),
            _ => None,
        }
    }
}

impl BackendBuilder for NapiBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Napi
    }

    fn feature(&self) -> &'static str {
        FEATURE
    }

    fn wrapper_ident(&self, data: &IndicatorData) -> Ident {
        format_ident!("Napi{}", data.rust_name)
    }

    fn validate_options(&self, options: &[BackendOption]) -> syn::Result<()> {
        no_options(self.kind(), options)
    }

    fn validate_method(&self, _data: &IndicatorData, method: &MethodData) -> syn::Result<()> {
        check_signature(&NapiMapping, &method.args, &method.output)
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
