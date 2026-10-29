use syn::spanned::Spanned;
use syn::{Data, DeriveInput};

use crate::ir::IndicatorData;
use crate::parser::attribute_parser::{attr_args, parse_binding_config};
use crate::parser::docs_parser::parse_docs;
use crate::validation::diagnostics::{Diagnostics, with_help};
use crate::validation::types::validate_indicator_generics;

/// Parses the indicator struct and its `#[bindings(...)]` configuration.
pub fn parse_struct(input: &DeriveInput) -> syn::Result<IndicatorData> {
    let mut diags = Diagnostics::new();

    if !matches!(input.data, Data::Struct(_)) {
        diags.error(
            input.ident.span(),
            "`#[derive(Bindings)]` only supports structs",
        );
    }

    let mut config_attrs = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("bindings"));
    let config = match config_attrs.next() {
        Some(attr) => {
            for extra in config_attrs {
                diags.error(
                    extra.span(),
                    "only one `#[bindings(...)]` attribute is allowed on the struct",
                );
            }
            diags
                .take(attr_args(attr))
                .and_then(|args| diags.take(parse_binding_config(&args, attr.span())))
        }
        None => {
            diags.push(with_help(
                input.ident.span(),
                format!(
                    "`{}` derives `Bindings` but has no `#[bindings(...)]` configuration",
                    input.ident
                ),
                "add `#[bindings(error = MyError)]` below the derive",
            ));
            None
        }
    };

    diags.take(validate_indicator_generics(&input.generics));

    let config = diags
        .finish_with(config)?
        .expect("missing config is reported above");
    let public_name = config
        .public_name
        .as_ref()
        .map(|name| name.value())
        .unwrap_or_else(|| input.ident.to_string());

    Ok(IndicatorData {
        rust_name: input.ident.clone(),
        public_name,
        vis: input.vis.clone(),
        generics: input.generics.clone(),
        config,
        docs: parse_docs(&input.attrs),
        constructor: None,
        update: None,
        reset: None,
        ready: None,
        custom: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::BackendKind;
    use syn::parse_quote;

    #[test]
    fn parses_struct_identity_and_config() {
        let input: DeriveInput = parse_quote! {
            /// Correlation Trend Indicator.
            #[derive(Bindings)]
            #[bindings(error = NovaError, backends(python), name = "CTI")]
            pub struct Cti<T: Num> { x: Window<T> }
        };
        let data = parse_struct(&input).unwrap();
        assert_eq!(data.rust_name, "Cti");
        assert_eq!(data.public_name, "CTI");
        assert_eq!(data.config.backends, [BackendKind::Python]);
        assert_eq!(data.generics.params.len(), 1);
        assert_eq!(
            data.docs.summary.as_deref(),
            Some("Correlation Trend Indicator.")
        );
        assert!(data.methods().next().is_none());
    }

    #[test]
    fn requires_configuration() {
        let input: DeriveInput = parse_quote!(
            struct Cti;
        );
        let error = parse_struct(&input).err().unwrap();
        assert!(
            error
                .to_string()
                .contains("has no `#[bindings(...)]` configuration")
        );
    }

    #[test]
    fn rejects_enums_lifetimes_and_const_generics() {
        let input: DeriveInput = parse_quote! {
            #[bindings(error = E)]
            enum Cti<'a, const N: usize> { A(&'a [f64; N]) }
        };
        let messages: Vec<String> = parse_struct(&input)
            .err()
            .unwrap()
            .into_iter()
            .map(|e| e.to_string())
            .collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].contains("only supports structs"));
        assert!(messages[1].contains("lifetime"));
        assert!(messages[2].contains("const"));
    }
}
