use syn::Path;
use zyn::TokenStream;

use super::{FEATURE, UniFfiBackend, UniFfiMapping};
use crate::backends::BackendBuilder;
use crate::backends::wrapper_fn::{Access, WrapperFn};
use crate::ir::{IndicatorData, MethodData, MethodKind};

/// One `#[uniffi::export]` impl block for the methods of one annotated block.
pub fn exported_impl(
    backend: &UniFfiBackend,
    data: &IndicatorData,
    wrapper: &Path,
    methods: &[&MethodData],
) -> TokenStream {
    let public_name = &data.public_name;
    let error_type = &data.config.error_type;
    let result_type =
        |ok: TokenStream| template! { ::core::result::Result<{{ ok }}, {{ error_type }}> };
    let functions: Vec<TokenStream> = methods
        .iter()
        .map(|method| {
            WrapperFn {
                map: &UniFfiMapping,
                numeric: backend.numeric_type(),
                access: Access::Mutex,
                name: method.binding_name.clone(),
                attrs: match method.kind {
                    MethodKind::Constructor => template! { #[uniffi::constructor] },
                    _ => TokenStream::new(),
                },
                map_error: &|error| error,
                result_type: &result_type,
                construct: (
                    template! { ::std::sync::Arc<Self> },
                    template! { ::std::sync::Arc::new(Self {
                        inner: ::std::sync::Mutex::new(__value)
                    }) },
                ),
            }
            .build(data, method)
        })
        .collect();
    template! {
        #[cfg(feature = {{ FEATURE }})]
        #[::uniffi::export(name = {{ public_name }})]
        impl {{ wrapper }} {
            @for (function in functions.iter()) { {{ function }} }
        }
    }
}
