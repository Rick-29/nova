//! The standard binding contract:
//!
//! | method   | receiver    | arguments                | returns                      |
//! |----------|-------------|--------------------------|------------------------------|
//! | `new`    | none        | indicator-specific       | `Self` or `Result<Self, E>`  |
//! | `update` | `&mut self` | one scalar numeric input | `T` or `Result<T, E>`        |
//! | `reset`  | `&mut self` | none                     | `()`                         |
//! | `ready`  | `&self`     | none                     | `bool`                       |
//!
//! Overrides keep the receiver and return-shape rules but may use any
//! supported argument and output types.

use syn::spanned::Spanned;

use crate::ir::{BindingType, MethodData, MethodKind, Receiver, ScalarType};
use crate::validation::diagnostics::{Diagnostics, with_help};
use crate::validation::types::check_supported;

pub fn validate_method(method: &MethodData) -> syn::Result<()> {
    let mut diags = Diagnostics::new();
    let sig = &method.signature;
    let name = &method.binding_name;

    if !sig.generics.params.is_empty() {
        diags.error_spanned(
            &sig.generics,
            "bound methods cannot have their own generic parameters",
        );
    }
    if let Some(asyncness) = &sig.asyncness {
        diags.error(asyncness.span(), "async methods are not supported");
    }
    if let Some(unsafety) = &sig.unsafety {
        diags.error(unsafety.span(), "unsafe methods cannot be bound");
    }
    if let Some(variadic) = &sig.variadic {
        diags.error_spanned(variadic, "variadic methods cannot be bound");
    }

    for arg in &method.args {
        check_supported(&arg.ty, &mut diags);
        if arg.ty.contains_self() {
            diags.error(
                arg.span,
                "bound methods cannot take the indicator itself as an argument",
            );
        }
    }
    check_supported(&method.output.ok, &mut diags);

    let expected_receiver = match method.kind {
        MethodKind::Constructor => Some(Receiver::None),
        MethodKind::Update | MethodKind::Reset => Some(Receiver::RefMut),
        MethodKind::Ready => Some(Receiver::Ref),
        MethodKind::Custom => None,
    };
    if let Some(expected) = expected_receiver
        && method.receiver != expected
    {
        let wanted = match expected {
            Receiver::None => "no `self` receiver",
            Receiver::Ref => "a `&self` receiver",
            Receiver::RefMut => "a `&mut self` receiver",
        };
        diags.error(
            sig.ident.span(),
            format!("binding method `{name}` must take {wanted}"),
        );
    }

    match method.kind {
        MethodKind::Constructor => {
            if !matches!(method.output.ok, BindingType::SelfType) {
                diags.push(with_help(
                    method.output.span,
                    "the binding constructor must return the indicator",
                    "return `Self` or `Result<Self, E>`",
                ));
            }
        }
        MethodKind::Update => {
            if !method.is_override {
                validate_standard_update(method, &mut diags);
            }
            if method.output.ok.contains_self() {
                diags.error(method.output.span, "`update` cannot return the indicator");
            }
        }
        MethodKind::Reset => {
            if !method.args.is_empty() {
                diags.error(method.args[0].span, "`reset` takes no arguments");
            }
            if !matches!(method.output.ok, BindingType::Unit) || method.output.fallible {
                diags.error(method.output.span, "`reset` must return `()`");
            }
        }
        MethodKind::Ready => {
            if !method.args.is_empty() {
                diags.error(method.args[0].span, "`ready` takes no arguments");
            }
            if !matches!(method.output.ok, BindingType::Scalar(ScalarType::Bool))
                || method.output.fallible
            {
                diags.push(with_help(
                    method.output.span,
                    "`ready` must return `bool`",
                    "readiness is always delegated to the indicator's own `ready`",
                ));
            }
        }
        MethodKind::Custom => {
            if method.receiver == Receiver::None {
                diags.push(with_help(
                    sig.ident.span(),
                    format!("exported method `{name}` needs a `&self` or `&mut self` receiver"),
                    "to bind a different constructor, mark it `#[bindings(constructor)]` or override `new`",
                ));
            }
            if method.output.ok.contains_self() {
                diags.error(
                    method.output.span,
                    "exported methods cannot return the indicator",
                );
            }
        }
    }

    diags.finish()
}

fn validate_standard_update(method: &MethodData, diags: &mut Diagnostics) {
    let scalar_help = "the standard `update` binding takes and returns one scalar numeric value \
                       (a type parameter or `f64`); bind other shapes with `overrides(update)`";
    match method.args.as_slice() {
        [input] if input.ty.is_float_scalar() => {}
        [input] if input.ty.find_unsupported().is_none() => diags.push(with_help(
            input.span,
            format!("unsupported `update` input `{}`", input.ty),
            scalar_help,
        )),
        [_] => {}
        args => diags.push(with_help(
            args.get(1)
                .map(|arg| arg.span)
                .unwrap_or(method.signature.span()),
            format!("`update` must take exactly one input, found {}", args.len()),
            scalar_help,
        )),
    }
    if !method.output.ok.is_float_scalar() && method.output.ok.find_unsupported().is_none() {
        diags.push(with_help(
            method.output.span,
            format!("unsupported `update` output `{}`", method.output.ok),
            scalar_help,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{CallTarget, Docs, ReturnData};
    use crate::parser::signature_parser::{TypeContext, parse_signature};
    use std::collections::HashMap;
    use syn::{Ident, parse_quote};

    fn method(kind: MethodKind, sig: syn::Signature) -> MethodData {
        let name: Ident = parse_quote!(Cti);
        let ctx = TypeContext {
            struct_name: &name,
            type_params: vec![parse_quote!(T)],
            assoc_types: HashMap::from([("Input".to_string(), parse_quote!(Ohlc<T>))]),
        };
        let parsed = parse_signature(&sig, &ctx).unwrap();
        MethodData {
            rust_name: sig.ident.clone(),
            binding_name: sig.ident.clone(),
            signature: sig,
            docs: Docs::default(),
            kind,
            is_override: false,
            override_backends: Vec::new(),
            receiver: parsed.receiver,
            args: parsed.args,
            output: ReturnData { ..parsed.output },
            call: CallTarget {
                self_ty: parse_quote!(Cti<T>),
                impl_generics: parse_quote!(<T>),
                trait_path: None,
            },
            span: zyn::Span::call_site(),
        }
    }

    fn errors(method: MethodData) -> Vec<String> {
        validate_method(&method)
            .err()
            .map(|e| e.into_iter().map(|e| e.to_string()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn accepts_the_standard_contract() {
        assert!(
            errors(method(
                MethodKind::Constructor,
                parse_quote!(fn new(period: usize, alpha: T) -> NovaResult<Self>)
            ))
            .is_empty()
        );
        assert!(
            errors(method(
                MethodKind::Update,
                parse_quote!(fn update(&mut self, input: T) -> Result<T, E>)
            ))
            .is_empty()
        );
        assert!(errors(method(MethodKind::Reset, parse_quote!(fn reset(&mut self)))).is_empty());
        assert!(
            errors(method(
                MethodKind::Ready,
                parse_quote!(fn ready(&self) -> bool)
            ))
            .is_empty()
        );
    }

    #[test]
    fn rejects_bad_constructors() {
        let errs = errors(method(
            MethodKind::Constructor,
            parse_quote!(fn new(period: usize) -> NovaResult<usize>),
        ));
        assert!(errs[0].contains("must return the indicator"));
        let errs = errors(method(
            MethodKind::Constructor,
            parse_quote!(fn new(name: &str) -> Self),
        ));
        assert!(errs[0].contains("cannot cross the binding boundary"));
    }

    #[test]
    fn rejects_non_scalar_update() {
        let errs = errors(method(
            MethodKind::Update,
            parse_quote!(fn update(&mut self, input: Self::Input) -> Result<T, E>),
        ));
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("`Ohlc<T>` cannot cross"));
        let errs = errors(method(
            MethodKind::Update,
            parse_quote!(fn update(&mut self, input: T) -> Result<Vec<T>, E>),
        ));
        assert!(errs[0].contains("unsupported `update` output"));
        let errs = errors(method(
            MethodKind::Update,
            parse_quote!(fn update(&self, a: T, b: T) -> T),
        ));
        assert_eq!(errs.len(), 2, "{errs:?}");
    }

    #[test]
    fn overrides_may_use_other_shapes() {
        let mut m = method(
            MethodKind::Update,
            parse_quote!(fn update_batch(&mut self, values: Vec<T>) -> Result<Vec<T>, E>),
        );
        m.is_override = true;
        assert!(errors(m).is_empty());
    }

    #[test]
    fn rejects_bad_reset_and_ready() {
        assert!(
            errors(method(
                MethodKind::Reset,
                parse_quote!(fn reset(&mut self) -> bool)
            ))[0]
                .contains("must return `()`")
        );
        assert!(
            errors(method(
                MethodKind::Ready,
                parse_quote!(fn ready(&self) -> Result<bool, E>)
            ))[0]
                .contains("must return `bool`")
        );
        assert!(
            errors(method(
                MethodKind::Ready,
                parse_quote!(fn ready(&mut self) -> bool)
            ))[0]
                .contains("`&self`")
        );
    }

    #[test]
    fn rejects_generic_and_async_methods() {
        let errs = errors(method(
            MethodKind::Custom,
            parse_quote!(async fn f<U>(&self)),
        ));
        assert_eq!(errs.len(), 2);
        let errs = errors(method(MethodKind::Custom, parse_quote!(fn f() -> f64)));
        assert!(errs[0].contains("receiver"));
    }
}
