use syn::Path;
use zyn::TokenStream;

use super::{FEATURE, NapiBackend, NapiMapping};
use crate::backends::wrapper_fn::{Access, WrapperFn};
use crate::backends::{BackendBuilder, doc_attrs};
use crate::codegen::delegate::concrete_indicator;
use crate::ir::{IndicatorData, MethodData, MethodKind};

pub fn wrapper_struct(backend: &NapiBackend, data: &IndicatorData) -> TokenStream {
    let wrapper = backend.wrapper_ident(data);
    let js_name = &data.public_name;
    let inner = concrete_indicator(data, &backend.numeric_type());
    let vis = &data.vis;
    let docs = doc_attrs(&data.docs);
    template! {
        #[cfg(feature = {{ FEATURE }})]
        #[::napi_derive::napi(js_name = {{ js_name }})]
        {{ docs }}
        {{ vis }} struct {{ wrapper }} {
            pub(crate) inner: {{ inner }},
        }
    }
}

pub fn exported_impl(
    backend: &NapiBackend,
    data: &IndicatorData,
    wrapper: &Path,
    methods: &[&MethodData],
) -> TokenStream {
    let functions: Vec<TokenStream> = methods
        .iter()
        .map(|method| {
            WrapperFn {
                map: &NapiMapping,
                numeric: backend.numeric_type(),
                access: Access::Direct,
                name: method.binding_name.clone(),
                attrs: match method.kind {
                    MethodKind::Constructor => template! { #[napi(constructor)] },
                    _ => template! { #[napi] },
                },
                map_error: &|error| {
                    template! { ::napi::Error::from_reason(
                        ::std::string::ToString::to_string(&{{ error }})
                    ) }
                },
                result_type: &|ok| template! { ::napi::Result<{{ ok }}> },
                construct: (template! { Self }, template! { Self { inner: __value } }),
            }
            .build(data, method)
        })
        .collect();
    template! {
        #[cfg(feature = {{ FEATURE }})]
        #[::napi_derive::napi]
        impl {{ wrapper }} {
            @for (function in functions.iter()) { {{ function }} }
        }
    }
}
