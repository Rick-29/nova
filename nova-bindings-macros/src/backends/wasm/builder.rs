use syn::Path;
use zyn::{TokenStream, format_ident};

use super::{FEATURE, WasmBackend, WasmMapping};
use crate::backends::wrapper_fn::{Access, WrapperFn};
use crate::backends::{BackendBuilder, doc_attrs};
use crate::codegen::delegate::concrete_indicator;
use crate::ir::{IndicatorData, MethodData, MethodKind};

pub fn wrapper_struct(backend: &WasmBackend, data: &IndicatorData) -> TokenStream {
    let wrapper = backend.wrapper_ident(data);
    let js_name = format_ident!("{}", data.public_name);
    let inner = concrete_indicator(data, &backend.numeric_type());
    let vis = &data.vis;
    let docs = doc_attrs(&data.docs);
    template! {
        #[cfg(feature = {{ FEATURE }})]
        #[::wasm_bindgen::prelude::wasm_bindgen(js_name = {{ js_name }})]
        {{ docs }}
        {{ vis }} struct {{ wrapper }} {
            pub(crate) inner: {{ inner }},
        }
    }
}

pub fn exported_impl(
    backend: &WasmBackend,
    data: &IndicatorData,
    wrapper: &Path,
    methods: &[&MethodData],
) -> TokenStream {
    let js_class = format_ident!("{}", data.public_name);
    let functions: Vec<TokenStream> = methods
        .iter()
        .map(|method| {
            WrapperFn {
                map: &WasmMapping,
                numeric: backend.numeric_type(),
                access: Access::Direct,
                name: method.binding_name.clone(),
                attrs: match method.kind {
                    MethodKind::Constructor => template! { #[wasm_bindgen(constructor)] },
                    _ => TokenStream::new(),
                },
                map_error: &|error| template! { ::wasm_bindgen::JsError::new(&::std::string::ToString::to_string(&{{ error }})) },
                result_type: &|ok| template! { ::core::result::Result<{{ ok }}, ::wasm_bindgen::JsError> },
                construct: (template! { Self }, template! { Self { inner: __value } }),
            }
            .build(data, method)
        })
        .collect();
    template! {
        #[cfg(feature = {{ FEATURE }})]
        #[::wasm_bindgen::prelude::wasm_bindgen(js_class = {{ js_class }})]
        impl {{ wrapper }} {
            @for (function in functions.iter()) { {{ function }} }
        }
    }
}
