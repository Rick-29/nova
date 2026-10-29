use syn::{Ident, Path};
use zyn::{TokenStream, format_ident};

use super::{FEATURE, MAPPING, PythonBackend, PythonOptions, STUBS_FEATURE, UpdatePolicy};
use crate::backends::BackendBuilder;
use crate::backends::wrapper_fn::{Access, WrapperFn};
use crate::codegen::braced;
use crate::ir::{IndicatorData, MethodData, MethodKind};

/// One `#[pymethods]` block for the methods of one annotated impl block.
pub fn pymethods(
    backend: &PythonBackend,
    data: &IndicatorData,
    wrapper: &Path,
    methods: &[&MethodData],
    options: &PythonOptions,
) -> TokenStream {
    let mut functions = TokenStream::new();
    for method in methods {
        let mut names = Vec::new();
        if method.kind != MethodKind::Update || options.update != UpdatePolicy::Call {
            names.push(method.binding_name.clone());
        }
        if method.kind == MethodKind::Update && options.update != UpdatePolicy::Method {
            names.push(format_ident!("__call__"));
        }
        for name in names {
            functions.extend(function(backend, data, method, name));
        }
    }
    let body = braced(functions);
    template! {
        #[cfg(feature = {{ FEATURE }})]
        #[cfg_attr(feature = {{ STUBS_FEATURE }}, ::pyo3_stub_gen::derive::gen_stub_pymethods)]
        #[::pyo3::pymethods]
        impl {{ wrapper }} {{ body }}
    }
}

fn function(
    backend: &PythonBackend,
    data: &IndicatorData,
    method: &MethodData,
    name: Ident,
) -> TokenStream {
    let arg_names: Vec<&Ident> = method.args.iter().map(|arg| &arg.name).collect();
    let constructor = method.kind == MethodKind::Constructor;
    let attrs = template! {
        @if (constructor) { #[new] }
        #[pyo3(signature = (@for (arg in arg_names.iter()) { {{ arg }}, }))]
    };
    WrapperFn {
        map: &MAPPING,
        numeric: backend.numeric_type(),
        access: Access::Direct,
        name,
        attrs,
        map_error: &|error| template! { ::pyo3::PyErr::from({{ error }}) },
        result_type: &|ok| template! { ::pyo3::PyResult<{{ ok }}> },
        construct: (template! { Self }, template! { Self { inner: __value } }),
    }
    .build(data, method)
}
