//! Mapping IR types to the Rust types a backend exposes, and converting
//! values across that boundary.

use syn::Type;
use syn::spanned::Spanned;
use zyn::{TokenStream, format_ident};

use crate::ir::{ArgData, BindingType, ScalarType};

/// How a backend represents scalars at its boundary.
pub trait ScalarMapping {
    fn backend(&self) -> &'static str;

    /// The boundary type for `scalar`, or a hint when it is unsupported.
    fn foreign(&self, scalar: ScalarType) -> Result<ScalarType, &'static str>;

    /// The boundary type for `scalar` in return position.
    fn foreign_out(&self, scalar: ScalarType) -> Result<ScalarType, &'static str> {
        self.foreign(scalar)
    }

    /// Converts a boundary value into the Rust scalar. `None` means identity.
    fn lift(&self, scalar: ScalarType, value: TokenStream) -> Option<TokenStream> {
        let _ = (scalar, value);
        None
    }

    /// Converts a Rust scalar into its boundary value. `None` means identity.
    fn lower(&self, scalar: ScalarType, value: TokenStream) -> Option<TokenStream> {
        let _ = (scalar, value);
        None
    }

    /// Extra restriction on `Vec<inner>`; `Err` carries a hint.
    fn check_vec(&self, inner: &BindingType) -> Result<(), &'static str> {
        let _ = inner;
        Ok(())
    }
}

/// A mapping where every scalar crosses unchanged.
pub struct Identity(pub &'static str);

impl ScalarMapping for Identity {
    fn backend(&self) -> &'static str {
        self.0
    }

    fn foreign(&self, scalar: ScalarType) -> Result<ScalarType, &'static str> {
        Ok(scalar)
    }
}

fn scalar_tokens(scalar: ScalarType) -> TokenStream {
    let ident = format_ident!("{}", scalar.name());
    template! { {{ ident }} }
}

/// The Rust type of `ty` on the indicator side.
pub fn rust_type(ty: &BindingType, numeric: &Type) -> TokenStream {
    match ty {
        BindingType::Unit => template! { () },
        BindingType::Scalar(scalar) => scalar_tokens(*scalar),
        BindingType::Numeric => template! { {{ numeric }} },
        BindingType::String => template! { ::std::string::String },
        BindingType::Vec(inner) => {
            let inner = rust_type(inner, numeric);
            template! { ::std::vec::Vec<{{ inner }}> }
        }
        BindingType::Option(inner) => {
            let inner = rust_type(inner, numeric);
            template! { ::core::option::Option<{{ inner }}> }
        }
        BindingType::SelfType | BindingType::Unsupported(_) => {
            unreachable!("rejected by validation before code generation")
        }
    }
}

/// Whether a type is an argument or a return value.
#[derive(Clone, Copy)]
pub enum Direction {
    In,
    Out,
}

/// The type of `ty` at the backend boundary.
pub fn foreign_type(
    map: &dyn ScalarMapping,
    ty: &BindingType,
    numeric: &Type,
    direction: Direction,
) -> TokenStream {
    match ty {
        BindingType::Scalar(scalar) => {
            let mapped = match direction {
                Direction::In => map.foreign(*scalar),
                Direction::Out => map.foreign_out(*scalar),
            };
            scalar_tokens(mapped.expect("validated"))
        }
        BindingType::Vec(inner) => {
            let inner = foreign_type(map, inner, numeric, direction);
            template! { ::std::vec::Vec<{{ inner }}> }
        }
        BindingType::Option(inner) => {
            let inner = foreign_type(map, inner, numeric, direction);
            template! { ::core::option::Option<{{ inner }}> }
        }
        other => rust_type(other, numeric),
    }
}

/// Reports types `map` cannot represent.
pub fn check_type(
    map: &dyn ScalarMapping,
    ty: &BindingType,
    span: zyn::Span,
    direction: Direction,
) -> syn::Result<()> {
    let unsupported = |hint: &str| {
        crate::validation::diagnostics::with_help(
            span,
            format!(
                "type `{ty}` is not supported by the `{}` backend",
                map.backend()
            ),
            hint,
        )
    };
    match ty {
        BindingType::Scalar(scalar) => match direction {
            Direction::In => map.foreign(*scalar),
            Direction::Out => map.foreign_out(*scalar),
        }
        .map(|_| ())
        .map_err(unsupported),
        BindingType::Vec(inner) => {
            map.check_vec(inner).map_err(unsupported)?;
            check_type(map, inner, span, direction)
        }
        BindingType::Option(inner) => check_type(map, inner, span, direction),
        _ => Ok(()),
    }
}

pub fn check_signature(
    map: &dyn ScalarMapping,
    args: &[ArgData],
    output: &crate::ir::ReturnData,
) -> syn::Result<()> {
    let mut diags = crate::validation::diagnostics::Diagnostics::new();
    for arg in args {
        diags.take(check_type(map, &arg.ty, arg.rust_ty.span(), Direction::In));
    }
    diags.take(check_type(map, &output.ok, output.span, Direction::Out));
    diags.finish()
}

fn convert(
    ty: &BindingType,
    value: TokenStream,
    scalar: &dyn Fn(ScalarType, TokenStream) -> Option<TokenStream>,
) -> Option<TokenStream> {
    match ty {
        BindingType::Scalar(s) => scalar(*s, value),
        BindingType::Vec(inner) => {
            let item = convert(inner, template! { __item }, scalar)?;
            Some(template! {
               {{ value }}
                   .into_iter()
                   .map(|__item| {{ item }})
                   .collect::<::std::vec::Vec<_>>()
            })
        }
        BindingType::Option(inner) => {
            let item = convert(inner, template! { __item }, scalar)?;
            Some(template! { {{ value }}.map(|__item| {{ item }}) })
        }
        _ => None,
    }
}

/// Boundary value → indicator value.
pub fn lift(map: &dyn ScalarMapping, ty: &BindingType, value: TokenStream) -> TokenStream {
    convert(ty, value.clone(), &|scalar, value| map.lift(scalar, value)).unwrap_or(value)
}

/// Indicator value → boundary value.
pub fn lower(map: &dyn ScalarMapping, ty: &BindingType, value: TokenStream) -> TokenStream {
    convert(ty, value.clone(), &|scalar, value| map.lower(scalar, value)).unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    struct Wide;
    impl ScalarMapping for Wide {
        fn backend(&self) -> &'static str {
            "test"
        }
        fn foreign(&self, scalar: ScalarType) -> Result<ScalarType, &'static str> {
            match scalar {
                ScalarType::Usize => Ok(ScalarType::U64),
                ScalarType::F32 => Err("use f64"),
                other => Ok(other),
            }
        }
        fn lift(&self, scalar: ScalarType, value: TokenStream) -> Option<TokenStream> {
            (scalar == ScalarType::Usize).then(|| template! { lift_usize({{ value }}) })
        }
    }

    #[test]
    fn maps_types_and_values() {
        let numeric: Type = parse_quote!(f64);
        let ty = BindingType::Vec(Box::new(BindingType::Scalar(ScalarType::Usize)));
        let text = |tokens: TokenStream| tokens.to_string().replace(' ', "");
        assert_eq!(
            text(foreign_type(&Wide, &ty, &numeric, Direction::In)),
            "::std::vec::Vec<u64>"
        );
        assert_eq!(text(rust_type(&ty, &numeric)), "::std::vec::Vec<usize>");
        assert_eq!(
            text(lift(&Wide, &ty, template! { values })),
            "values.into_iter().map(|__item|lift_usize(__item)).collect::<::std::vec::Vec<_>>()"
        );
        let plain = BindingType::Option(Box::new(BindingType::Numeric));
        assert_eq!(text(lift(&Wide, &plain, template! { x })), "x");
        assert_eq!(
            text(foreign_type(&Wide, &plain, &numeric, Direction::Out)),
            "::core::option::Option<f64>"
        );
    }

    #[test]
    fn reports_unsupported_scalars() {
        let ty = BindingType::Option(Box::new(BindingType::Scalar(ScalarType::F32)));
        let error = check_type(&Wide, &ty, zyn::Span::call_site(), Direction::In).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("not supported by the `test` backend")
        );
        assert!(error.to_string().contains("use f64"));
    }
}
