//! Delegation to the original indicator code.
//!
//! Generated wrappers never contain indicator logic: they call the annotated
//! constructor and methods on the concrete instantiation of the indicator.

use syn::visit_mut::VisitMut;
use syn::{GenericParam, Generics, Ident, Path, Type};
use zyn::TokenStream;

use crate::ir::{IndicatorData, MethodData};

struct Instantiate<'a> {
    params: &'a [Ident],
    concrete: &'a Type,
}

impl VisitMut for Instantiate<'_> {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if let Type::Path(path) = ty
            && path.qself.is_none()
            && let Some(ident) = path.path.get_ident()
            && self.params.contains(ident)
        {
            *ty = self.concrete.clone();
            return;
        }
        syn::visit_mut::visit_type_mut(self, ty);
    }
}

fn type_params(generics: &Generics) -> Vec<Ident> {
    generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Type(param) => Some(param.ident.clone()),
            _ => None,
        })
        .collect()
}

/// Replaces every type parameter of `generics` in `ty` with `concrete`.
pub fn instantiate_type(ty: &Type, generics: &Generics, concrete: &Type) -> Type {
    let params = type_params(generics);
    let mut ty = ty.clone();
    Instantiate {
        params: &params,
        concrete,
    }
    .visit_type_mut(&mut ty);
    ty
}

pub fn instantiate_path(path: &Path, generics: &Generics, concrete: &Type) -> Path {
    let params = type_params(generics);
    let mut path = path.clone();
    Instantiate {
        params: &params,
        concrete,
    }
    .visit_path_mut(&mut path);
    path
}

/// The indicator type stored in a wrapper, e.g. `Cti<f64>`.
pub fn concrete_indicator(data: &IndicatorData, numeric: &Type) -> Type {
    let name = &data.rust_name;
    let params = type_params(&data.generics);
    let ty: TokenStream = if params.is_empty() {
        template! { {{ name }} }
    } else {
        template! { {{ name }}< @for (_param in params.iter()) { {{ numeric }}, } > }
    };
    syn::parse2(ty).expect("indicator type is valid")
}

/// Calls `method` on the concrete indicator:
/// `<Cti<f64> as Indicator>::update(receiver, args...)`.
pub fn call(
    method: &MethodData,
    numeric: &Type,
    receiver: Option<TokenStream>,
    args: &[TokenStream],
) -> TokenStream {
    let generics = &method.call.impl_generics;
    let self_ty = instantiate_type(&method.call.self_ty, generics, numeric);
    let name = &method.rust_name;
    let all_args: Vec<TokenStream> = receiver.into_iter().chain(args.iter().cloned()).collect();
    match &method.call.trait_path {
        Some(trait_path) => {
            let trait_path = instantiate_path(trait_path, generics, numeric);
            template! { <{{ self_ty }} as {{ trait_path }}>::{{ name }}(@for (arg in all_args.iter()) { {{ arg }}, }) }
        }
        None => {
            template! { <{{ self_ty }}>::{{ name }}(@for (arg in all_args.iter()) { {{ arg }}, }) }
        }
    }
}

/// Evaluates `call`. For fallible methods the error is converted into the
/// configured error type (a compiler-checked `From` conversion), mapped by
/// `map_error`, and returned early.
pub fn unwrap_or_return(
    call: TokenStream,
    fallible: bool,
    error_type: &Type,
    map_error: impl FnOnce(TokenStream) -> TokenStream,
) -> TokenStream {
    if !fallible {
        return call;
    }
    let converted = template! { <{{ error_type }} as ::core::convert::From<_>>::from(__error) };
    let mapped = map_error(converted);
    template! {
        match {{ call }} {
            ::core::result::Result::Ok(__value) => __value,
            ::core::result::Result::Err(__error) => return ::core::result::Result::Err({{ mapped }}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BindingType, CallTarget, Docs, MethodKind, Receiver, ReturnData};
    use syn::parse_quote;

    fn method(trait_path: Option<Path>) -> MethodData {
        MethodData {
            rust_name: parse_quote!(update),
            binding_name: parse_quote!(update),
            signature: parse_quote!(fn update(&mut self, input: T) -> T),
            docs: Docs::default(),
            kind: MethodKind::Update,
            is_override: false,
            override_backends: Vec::new(),
            receiver: Receiver::RefMut,
            args: Vec::new(),
            output: ReturnData {
                ok: BindingType::Numeric,
                fallible: false,
                span: zyn::Span::call_site(),
            },
            call: CallTarget {
                self_ty: parse_quote!(Cti<T>),
                impl_generics: parse_quote!(<T: Num>),
                trait_path,
            },
            span: zyn::Span::call_site(),
        }
    }

    #[test]
    fn instantiates_type_parameters() {
        let ty = instantiate_type(
            &parse_quote!(Pair<T, Vec<T>, U>),
            &parse_quote!(<T>),
            &parse_quote!(f64),
        );
        assert_eq!(
            zyn::ToTokens::to_token_stream(&ty).to_string(),
            "Pair < f64 , Vec < f64 > , U >"
        );
    }

    #[test]
    fn calls_trait_and_inherent_methods() {
        let numeric: Type = parse_quote!(f64);
        let args = [template! { input }];
        let trait_call = call(
            &method(Some(parse_quote!(Indicator))),
            &numeric,
            Some(template! { &mut self.inner }),
            &args,
        );
        assert_eq!(
            trait_call.to_string().replace(' ', ""),
            "<Cti<f64>asIndicator>::update(&mutself.inner,input,)"
        );
        let inherent = call(&method(None), &numeric, None, &args);
        assert_eq!(
            inherent.to_string().replace(' ', ""),
            "<Cti<f64>>::update(input,)"
        );
    }

    #[test]
    fn fallible_calls_convert_errors() {
        let error: Type = parse_quote!(NovaError);
        let code = unwrap_or_return(
            template! { f() },
            true,
            &error,
            |e| template! { wrap({{ e }}) },
        );
        let text = code.to_string().replace(' ', "");
        assert!(
            text.contains("<NovaErroras::core::convert::From<_>>::from(__error)"),
            "{text}"
        );
        assert!(
            text.contains("return::core::result::Result::Err(wrap("),
            "{text}"
        );
        assert_eq!(
            unwrap_or_return(template! { f() }, false, &error, |e| e).to_string(),
            "f ()"
        );
    }
}
