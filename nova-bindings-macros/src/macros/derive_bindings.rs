use syn::DeriveInput;
use zyn::{ToTokens, TokenStream};

use crate::backends::factory;
use crate::codegen::carrier::carrier_macro;
use crate::codegen::markers::markers;
use crate::ir::IndicatorData;
use crate::parser::struct_parser::parse_struct;
use crate::validation::diagnostics::Diagnostics;

/// `#[derive(Bindings)]`: wrapper types, completeness markers and the
/// configuration carrier.
pub fn expand(input: TokenStream) -> TokenStream {
    let input: DeriveInput = match syn::parse2(input) {
        Ok(input) => input,
        Err(error) => return error.to_compile_error(),
    };
    match build(&input) {
        Ok(tokens) => tokens,
        Err(error) => {
            let mut tokens = error.to_compile_error();
            tokens.extend(carrier_macro(&input.ident, None));
            tokens
        }
    }
}

fn build(input: &DeriveInput) -> syn::Result<TokenStream> {
    let data = parse_struct(input)?;
    validate_options(&data)?;

    let mut tokens = markers(&data);
    tokens.extend(carrier_macro(
        &data.rust_name,
        Some(input.to_token_stream()),
    ));
    for builder in factory::selected(&data.config.backends) {
        tokens.extend(builder.build_struct(&data)?);
    }
    Ok(tokens)
}

/// Backend options are checked once, here; impl expansions assume them valid.
pub fn validate_options(data: &IndicatorData) -> syn::Result<()> {
    let mut diags = Diagnostics::new();
    for builder in factory::selected(&data.config.backends) {
        diags.take(builder.validate_options(data.config.options(builder.kind())));
    }
    diags.finish()
}
