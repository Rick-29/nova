use zyn::{TokenStream, format_ident};

use super::{FEATURE, PythonBackend, PythonErrorOptions, PythonOptions, STUBS_FEATURE};
use crate::backends::{BackendBuilder, ErrorGlue, doc_attrs};
use crate::codegen::delegate::concrete_indicator;
use crate::ir::{ErrorData, IndicatorData};

/// `#[pyclass(name = "Cti")] pub struct PyCti { inner: Cti<f64> }`.
pub fn wrapper_struct(
    backend: &PythonBackend,
    data: &IndicatorData,
    options: &PythonOptions,
) -> TokenStream {
    let wrapper = backend.wrapper_ident(data);
    let public_name = &data.public_name;
    let inner = concrete_indicator(data, &backend.numeric_type());
    let vis = &data.vis;
    let docs = doc_attrs(&data.docs);
    let module = options.module.as_ref();
    template! {
        #[cfg(feature = {{ FEATURE }})]
        #[cfg_attr(feature = {{ STUBS_FEATURE }}, ::pyo3_stub_gen::derive::gen_stub_pyclass)]
        #[::pyo3::pyclass(name = {{ public_name }} @if (module.is_some()) { , module = {{ module }} })]
        {{ docs }}
        {{ vis }} struct {{ wrapper }} {
            pub(crate) inner: {{ inner }},
        }
    }
}

/// A Python exception class for the error type plus the `PyErr` conversion
/// every generated wrapper relies on.
pub fn error_glue(error: &ErrorData, options: &PythonErrorOptions) -> ErrorGlue {
    let ident = &error.ident;
    let vis = &error.vis;
    let module_ident = format_ident!("__nova_bindings_python_{}", ident);
    let alias = format_ident!("Py{}", ident);
    let base = format_ident!("Py{}", options.base);
    let python_module = format_ident!(
        "{}",
        options
            .module
            .as_ref()
            .map(|module| module.value())
            .or_else(|| std::env::var("CARGO_CRATE_NAME").ok())
            .unwrap_or_else(|| "builtins".to_string())
    );
    let doc = match error.docs.text() {
        text if text.is_empty() => format!("Error raised by `{ident}` bindings."),
        text => text,
    };
    let items = template! {
        #[cfg(feature = {{ FEATURE }})]
        #[doc(hidden)]
        #[allow(non_snake_case)]
        {{ vis }} mod {{ module_ident }} {
            #[cfg(not(feature = {{ STUBS_FEATURE }}))]
            ::pyo3::create_exception!({{ python_module }}, {{ ident }}, ::pyo3::exceptions::{{ base }}, {{ doc }});
            #[cfg(feature = {{ STUBS_FEATURE }})]
            ::pyo3_stub_gen::create_exception!({{ python_module }}, {{ ident }}, ::pyo3::exceptions::{{ base }}, {{ doc }});
        }

        /// Python exception raised for this error type.
        #[cfg(feature = {{ FEATURE }})]
        {{ vis }} use {{ module_ident }}::{{ ident }} as {{ alias }};

        #[cfg(feature = {{ FEATURE }})]
        impl ::core::convert::From<{{ ident }}> for ::pyo3::PyErr {
            fn from(error: {{ ident }}) -> Self {
                {{ alias }}::new_err(::std::string::ToString::to_string(&error))
            }
        }
    };
    ErrorGlue {
        attrs: TokenStream::new(),
        items,
    }
}
