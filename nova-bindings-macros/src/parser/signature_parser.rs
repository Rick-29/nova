//! Method signatures, receivers and type classification.
//!
//! Classification is purely syntactic. Full type compatibility is left to
//! rustc through the generated code.

use std::collections::HashMap;

use syn::spanned::Spanned;
use syn::{FnArg, GenericArgument, Ident, Pat, PathArguments, ReturnType, Signature, Type};

use crate::ir::{ArgData, BindingType, Receiver, ReturnData, ScalarType};
use crate::validation::diagnostics::Diagnostics;

/// What a signature may refer to besides plain types.
pub struct TypeContext<'a> {
    /// Name of the annotated indicator struct.
    pub struct_name: &'a Ident,
    /// Type parameters of the impl block; they stand for the numeric type.
    pub type_params: Vec<Ident>,
    /// Associated types of a trait impl, e.g. `Input = T`.
    pub assoc_types: HashMap<String, Type>,
}

pub struct ParsedSignature {
    pub receiver: Receiver,
    pub args: Vec<ArgData>,
    pub output: ReturnData,
}

pub fn parse_signature(sig: &Signature, ctx: &TypeContext<'_>) -> syn::Result<ParsedSignature> {
    let mut diags = Diagnostics::new();
    let mut receiver = Receiver::None;
    let mut args = Vec::new();

    for input in &sig.inputs {
        match input {
            FnArg::Receiver(recv) => {
                if recv.reference.is_none() || recv.colon_token.is_some() {
                    diags
                        .error_spanned(recv, "only `&self` and `&mut self` receivers can be bound");
                } else if recv.mutability.is_some() {
                    receiver = Receiver::RefMut;
                } else {
                    receiver = Receiver::Ref;
                }
            }
            FnArg::Typed(arg) => {
                let name = match &*arg.pat {
                    Pat::Ident(pat) if pat.by_ref.is_none() && pat.subpat.is_none() => {
                        Some(pat.ident.clone())
                    }
                    other => {
                        diags.error_spanned(
                            other,
                            "bound arguments must be plain identifiers so their names can be exported",
                        );
                        None
                    }
                };
                if let Some(name) = name {
                    args.push(ArgData {
                        name,
                        ty: classify_type(&arg.ty, ctx),
                        rust_ty: (*arg.ty).clone(),
                        span: arg.span(),
                    });
                }
            }
        }
    }

    let output = parse_return(&sig.output, ctx);
    diags.finish_with(ParsedSignature {
        receiver,
        args,
        output,
    })
}

/// Classifies a return type, unwrapping `Result<T, E>` and `XResult<T>` aliases.
pub fn parse_return(output: &ReturnType, ctx: &TypeContext<'_>) -> ReturnData {
    let ReturnType::Type(_, ty) = output else {
        return ReturnData {
            ok: BindingType::Unit,
            fallible: false,
            span: output.span(),
        };
    };
    if let Type::Path(path) = &**ty
        && path.qself.is_none()
        && let Some(last) = path.path.segments.last()
        && last.ident.to_string().ends_with("Result")
        && let PathArguments::AngleBracketed(generics) = &last.arguments
        && let Some(GenericArgument::Type(ok)) = generics.args.first()
    {
        return ReturnData {
            ok: classify_type(ok, ctx),
            fallible: true,
            span: ty.span(),
        };
    }
    ReturnData {
        ok: classify_type(ty, ctx),
        fallible: false,
        span: ty.span(),
    }
}

pub fn classify_type(ty: &Type, ctx: &TypeContext<'_>) -> BindingType {
    let unsupported = || BindingType::Unsupported(Box::new(ty.clone()));
    match ty {
        Type::Paren(inner) => classify_type(&inner.elem, ctx),
        Type::Group(inner) => classify_type(&inner.elem, ctx),
        Type::Tuple(tuple) if tuple.elems.is_empty() => BindingType::Unit,
        Type::Path(path) => {
            // `<Self as Trait>::Input`
            if let Some(qself) = &path.qself {
                let is_self = matches!(&*qself.ty, Type::Path(p) if p.path.is_ident("Self"));
                let assoc = path.path.segments.last().map(|seg| seg.ident.to_string());
                return match (is_self, assoc) {
                    (true, Some(name)) => resolve_assoc(&name, ctx).unwrap_or_else(unsupported),
                    _ => unsupported(),
                };
            }
            let segments: Vec<_> = path.path.segments.iter().collect();
            // `Self::Input`
            if segments.len() == 2 && segments[0].ident == "Self" {
                return resolve_assoc(&segments[1].ident.to_string(), ctx)
                    .unwrap_or_else(unsupported);
            }
            let Some(last) = segments.last() else {
                return unsupported();
            };
            let ident = last.ident.to_string();
            if path.path.is_ident("Self") || last.ident == *ctx.struct_name {
                return BindingType::SelfType;
            }
            if segments.len() == 1 && last.arguments.is_none() {
                if ctx.type_params.iter().any(|param| param == &last.ident) {
                    return BindingType::Numeric;
                }
                if let Some(scalar) = ScalarType::from_name(&ident) {
                    return BindingType::Scalar(scalar);
                }
            }
            match (ident.as_str(), single_type_arg(&last.arguments)) {
                ("String", None) => BindingType::String,
                ("Vec", Some(inner)) => BindingType::Vec(Box::new(classify_type(inner, ctx))),
                ("Option", Some(inner)) => BindingType::Option(Box::new(classify_type(inner, ctx))),
                _ => unsupported(),
            }
        }
        _ => unsupported(),
    }
}

fn resolve_assoc(name: &str, ctx: &TypeContext<'_>) -> Option<BindingType> {
    ctx.assoc_types.get(name).map(|ty| classify_type(ty, ctx))
}

fn single_type_arg(arguments: &PathArguments) -> Option<&Type> {
    match arguments {
        PathArguments::AngleBracketed(generics) if generics.args.len() == 1 => {
            match generics.args.first() {
                Some(GenericArgument::Type(ty)) => Some(ty),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn ctx(name: &Ident) -> TypeContext<'_> {
        TypeContext {
            struct_name: name,
            type_params: vec![parse_quote!(T)],
            assoc_types: HashMap::from([
                ("Input".to_string(), parse_quote!(T)),
                ("Output".to_string(), parse_quote!(f64)),
            ]),
        }
    }

    fn classify(name: &Ident, ty: Type) -> String {
        classify_type(&ty, &ctx(name)).to_string()
    }

    #[test]
    fn classifies_types() {
        let name: Ident = parse_quote!(Cti);
        assert_eq!(classify(&name, parse_quote!(usize)), "usize");
        assert_eq!(classify(&name, parse_quote!(T)), "type parameter");
        assert_eq!(classify(&name, parse_quote!(Self::Input)), "type parameter");
        assert_eq!(
            classify(&name, parse_quote!(<Self as Indicator>::Output)),
            "f64"
        );
        assert_eq!(
            classify(&name, parse_quote!(std::vec::Vec<Option<T>>)),
            "Vec<Option<type parameter>>"
        );
        assert_eq!(classify(&name, parse_quote!(String)), "String");
        assert_eq!(classify(&name, parse_quote!(())), "()");
        assert_eq!(classify(&name, parse_quote!(Self)), "Self");
        assert_eq!(classify(&name, parse_quote!(Cti<T>)), "Self");
        assert!(matches!(
            classify_type(&parse_quote!(&str), &ctx(&name)),
            BindingType::Unsupported(_)
        ));
        assert!(matches!(
            classify_type(&parse_quote!(Ohlc<T>), &ctx(&name)),
            BindingType::Unsupported(_)
        ));
        assert!(matches!(
            classify_type(&parse_quote!(Self::Error), &ctx(&name)),
            BindingType::Unsupported(_)
        ));
    }

    #[test]
    fn parses_signatures() {
        let name: Ident = parse_quote!(Cti);
        let sig: Signature =
            parse_quote!(fn update(&mut self, mut input: Self::Input) -> NovaResult<Self::Output>);
        let parsed = parse_signature(&sig, &ctx(&name)).unwrap();
        assert_eq!(parsed.receiver, Receiver::RefMut);
        assert_eq!(parsed.args[0].name, "input");
        assert!(parsed.args[0].ty.is_float_scalar());
        assert!(parsed.output.fallible);
        assert!(parsed.output.ok.is_float_scalar());

        let sig: Signature = parse_quote!(fn ready(&self) -> bool);
        let parsed = parse_signature(&sig, &ctx(&name)).unwrap();
        assert_eq!(parsed.receiver, Receiver::Ref);
        assert!(!parsed.output.fallible);

        let sig: Signature = parse_quote!(fn reset(&mut self));
        assert!(matches!(
            parse_signature(&sig, &ctx(&name)).unwrap().output.ok,
            BindingType::Unit
        ));

        let sig: Signature = parse_quote!(fn new(period: usize) -> Result<Self, NovaError>);
        let parsed = parse_signature(&sig, &ctx(&name)).unwrap();
        assert_eq!(parsed.receiver, Receiver::None);
        assert!(matches!(parsed.output.ok, BindingType::SelfType));
    }

    #[test]
    fn rejects_unbindable_patterns_and_receivers() {
        let name: Ident = parse_quote!(Cti);
        let sig: Signature = parse_quote!(fn f(self: Box<Self>, (a, b): (f64, f64)));
        let messages: Vec<String> = parse_signature(&sig, &ctx(&name))
            .err()
            .unwrap()
            .into_iter()
            .map(|e| e.to_string())
            .collect();
        assert_eq!(messages.len(), 2);
    }
}
