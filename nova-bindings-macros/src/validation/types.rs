use syn::spanned::Spanned;
use syn::{GenericParam, Generics};

use crate::ir::{BindingType, display_type};
use crate::validation::diagnostics::{Diagnostics, with_help};

const SUPPORTED: &str = "bool, f32, f64, integer primitives, String, the indicator's type parameters, and Vec<_> / Option<_> of those";

/// Indicators are instantiated by each backend, so only type parameters are
/// allowed; rustc checks that the numeric type satisfies their bounds.
pub fn validate_indicator_generics(generics: &Generics) -> syn::Result<()> {
    let mut diags = Diagnostics::new();
    for param in &generics.params {
        match param {
            GenericParam::Type(_) => {}
            GenericParam::Lifetime(lifetime) => diags.error_spanned(
                lifetime,
                "indicators with lifetime parameters cannot be exported to foreign languages",
            ),
            GenericParam::Const(param) => diags.push(with_help(
                param.span(),
                "const generic parameters are not supported",
                "foreign bindings instantiate type parameters only; make the value a constructor argument",
            )),
        }
    }
    diags.finish()
}

/// Reports a type that no backend can export.
pub fn check_supported(ty: &BindingType, diags: &mut Diagnostics) {
    if let Some(unsupported) = ty.find_unsupported() {
        diags.push(with_help(
            unsupported.span(),
            format!(
                "type `{}` cannot cross the binding boundary",
                display_type(unsupported)
            ),
            format!("supported types: {SUPPORTED}"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn type_parameters_are_accepted() {
        let generics: Generics = parse_quote!(<T: Num, R: Real>);
        assert!(validate_indicator_generics(&generics).is_ok());
    }

    #[test]
    fn reports_unsupported_types() {
        let mut diags = Diagnostics::new();
        check_supported(
            &BindingType::Vec(Box::new(BindingType::Unsupported(Box::new(parse_quote!(
                Ohlc<T>
            ))))),
            &mut diags,
        );
        let error = diags.finish().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("`Ohlc<T>` cannot cross the binding boundary")
        );
    }
}
