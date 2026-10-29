//! PyO3 backend.
//!
//! `Cti<T>` becomes `PyCti`, a `#[pyclass(name = "Cti")]` holding a
//! `Cti<f64>`. Every `#[bindings]` impl block becomes its own `#[pymethods]`
//! block, so the crate must enable PyO3's `multiple-pymethods` feature. With
//! the `python-stubs` feature the items are also registered with
//! `pyo3-stub-gen`.

mod builder;
mod methods;

use syn::{Ident, LitStr, Path};
use zyn::{TokenStream, format_ident};

use crate::backends::types::{Identity, check_signature};
use crate::backends::{BackendBuilder, ErrorGlue};
use crate::ir::{BackendKind, BackendOption, ErrorData, IndicatorData, MethodData, OptionValue};
use crate::validation::diagnostics::{Diagnostics, unknown_name};

pub const FEATURE: &str = "python";
pub const STUBS_FEATURE: &str = "python-stubs";

const MAPPING: Identity = Identity("python");

pub struct PythonBackend;

/// How `update` is exposed to Python: `python(update = method | call | both)`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UpdatePolicy {
    /// `indicator.update(x)`.
    Method,
    /// `indicator(x)`.
    Call,
    /// Both forms.
    Both,
}

pub struct PythonOptions {
    pub module: Option<LitStr>,
    pub update: UpdatePolicy,
}

fn expect_str(option: &BackendOption) -> syn::Result<LitStr> {
    match &option.value {
        OptionValue::Str(text) => Ok(text.clone()),
        _ => Err(syn::Error::new(
            option.key.span(),
            format!("`{}` expects a string", option.key),
        )),
    }
}

fn expect_ident(option: &BackendOption) -> syn::Result<Ident> {
    match &option.value {
        OptionValue::Ident(ident) => Ok(ident.clone()),
        _ => Err(syn::Error::new(
            option.key.span(),
            format!("`{}` expects an identifier", option.key),
        )),
    }
}

fn module_name(option: &BackendOption) -> syn::Result<LitStr> {
    let module = expect_str(option)?;
    if syn::parse_str::<Ident>(&module.value()).is_err() {
        return Err(syn::Error::new(
            module.span(),
            "the Python module name must be an identifier",
        ));
    }
    Ok(module)
}

pub fn parse_options(options: &[BackendOption]) -> syn::Result<PythonOptions> {
    let mut diags = Diagnostics::new();
    let mut parsed = PythonOptions {
        module: None,
        update: UpdatePolicy::Method,
    };
    for option in options {
        match option.key.to_string().as_str() {
            "module" => parsed.module = diags.take(module_name(option)),
            "update" => {
                if let Some(value) = diags.take(expect_ident(option)) {
                    parsed.update = match value.to_string().as_str() {
                        "method" => UpdatePolicy::Method,
                        "call" => UpdatePolicy::Call,
                        "both" => UpdatePolicy::Both,
                        other => {
                            diags.push(unknown_name(
                                value.span(),
                                "Python update policy",
                                other,
                                &["method", "call", "both"],
                            ));
                            UpdatePolicy::Method
                        }
                    };
                }
            }
            other => diags.push(unknown_name(
                option.key.span(),
                "Python option",
                other,
                &["module", "update"],
            )),
        }
    }
    diags.finish_with(parsed)
}

/// Options of `#[bindings_error(python(...))]`.
pub struct PythonErrorOptions {
    pub module: Option<LitStr>,
    /// Python base exception, e.g. `ValueError`.
    pub base: Ident,
}

const EXCEPTION_BASES: [&str; 6] = [
    "Exception",
    "ValueError",
    "RuntimeError",
    "ArithmeticError",
    "TypeError",
    "IndexError",
];

pub fn parse_error_options(options: &[BackendOption]) -> syn::Result<PythonErrorOptions> {
    let mut diags = Diagnostics::new();
    let mut parsed = PythonErrorOptions {
        module: None,
        base: format_ident!("ValueError"),
    };
    for option in options {
        match option.key.to_string().as_str() {
            "module" => parsed.module = diags.take(module_name(option)),
            "base" => {
                if let Some(base) = diags.take(expect_ident(option)) {
                    if EXCEPTION_BASES.contains(&base.to_string().as_str()) {
                        parsed.base = base;
                    } else {
                        diags.push(unknown_name(
                            base.span(),
                            "Python exception base",
                            &base.to_string(),
                            &EXCEPTION_BASES,
                        ));
                    }
                }
            }
            other => diags.push(unknown_name(
                option.key.span(),
                "Python error option",
                other,
                &["module", "base"],
            )),
        }
    }
    diags.finish_with(parsed)
}

impl BackendBuilder for PythonBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Python
    }

    fn feature(&self) -> &'static str {
        FEATURE
    }

    fn wrapper_ident(&self, data: &IndicatorData) -> Ident {
        format_ident!("Py{}", data.rust_name)
    }

    fn validate_options(&self, options: &[BackendOption]) -> syn::Result<()> {
        parse_options(options).map(|_| ())
    }

    fn validate_method(&self, _data: &IndicatorData, method: &MethodData) -> syn::Result<()> {
        check_signature(&MAPPING, &method.args, &method.output)
    }

    fn build_struct(&self, data: &IndicatorData) -> syn::Result<TokenStream> {
        let options = parse_options(data.config.options(BackendKind::Python))?;
        Ok(builder::wrapper_struct(self, data, &options))
    }

    fn build_methods(
        &self,
        data: &IndicatorData,
        wrapper: &Path,
        methods: &[&MethodData],
    ) -> syn::Result<TokenStream> {
        let options = parse_options(data.config.options(BackendKind::Python))?;
        Ok(methods::pymethods(self, data, wrapper, methods, &options))
    }

    fn validate_error_options(&self, options: &[BackendOption]) -> syn::Result<()> {
        parse_error_options(options).map(|_| ())
    }

    fn build_error(&self, error: &ErrorData) -> syn::Result<ErrorGlue> {
        let options = parse_error_options(error.options(BackendKind::Python))?;
        Ok(builder::error_glue(error, &options))
    }
}
