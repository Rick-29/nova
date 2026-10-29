//! The configuration carrier.
//!
//! Procedural-macro invocations cannot share memory, so the derive emits a
//! hidden `macro_rules!` holding the struct declaration. Each `#[bindings]`
//! impl block expands to a call of that macro, which forwards the struct and
//! the impl block to `__bindings_impl!`. The compiler's own name resolution
//! connects them; no global state is involved.

use syn::{Ident, Path};
use zyn::{ToTokens, TokenStream};

use crate::codegen::braced;
use crate::codegen::naming::{carrier_ident, sibling};

/// The carrier definition. With `indicator = None` (the struct failed to
/// parse and errors were already reported) it swallows its input so impl
/// blocks do not pile on further errors.
pub fn carrier_macro(name: &Ident, indicator: Option<TokenStream>) -> TokenStream {
    let carrier = carrier_ident(name);
    let body = match indicator {
        Some(indicator) => {
            let indicator = braced(indicator);
            template! {
                ::nova_bindings_macros::__bindings_impl! { indicator {{ indicator }} $($tokens)* }
            }
        }
        None => TokenStream::new(),
    };
    let body = braced(body);
    template! {
        #[doc(hidden)]
        #[allow(unused_macros)]
        macro_rules! {{ carrier }} {
            ($($tokens:tt)*) => {{ body }};
        }
        #[doc(hidden)]
        #[allow(unused_imports)]
        pub(crate) use {{ carrier }};
    }
}

/// The call emitted by `#[bindings]` on an impl block.
pub fn carrier_call(target: &Path, args: TokenStream, item: TokenStream) -> TokenStream {
    let name = target
        .segments
        .last()
        .expect("target path is not empty")
        .ident
        .clone();
    let carrier = sibling(target, carrier_ident(&name));
    let target = braced(target.to_token_stream());
    let args = braced(args);
    let item = braced(item);
    template! {
        {{ carrier }}! { target {{ target }} args {{ args }} item {{ item }} }
    }
}
