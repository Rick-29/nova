use syn::Item;
use zyn::{ToTokens, TokenStream};

use crate::backends::factory;
use crate::parser::attribute_parser::{parse_args, parse_error_args};
use crate::parser::error_parser::parse_error;
use crate::validation::diagnostics::Diagnostics;

/// `#[bindings_error]`: per-backend conversions for the exported error type,
/// generated once so every indicator wrapper can rely on them.
pub fn expand(args: TokenStream, item: TokenStream) -> TokenStream {
    let item: Item = match syn::parse2(item) {
        Ok(item) => item,
        Err(error) => return error.to_compile_error(),
    };
    match build(args, &item) {
        Ok(tokens) => tokens,
        Err(error) => {
            let mut tokens = error.to_compile_error();
            tokens.extend(item.to_token_stream());
            tokens
        }
    }
}

fn build(args: TokenStream, item: &Item) -> syn::Result<TokenStream> {
    let args = parse_error_args(&parse_args(args)?)?;
    let error = parse_error(item, args)?;

    let mut diags = Diagnostics::new();
    let mut attrs = TokenStream::new();
    let mut items = TokenStream::new();
    for builder in factory::selected(&error.backends) {
        if diags
            .take(builder.validate_error_options(error.options(builder.kind())))
            .is_none()
        {
            continue;
        }
        if let Some(glue) = diags.take(builder.build_error(&error)) {
            attrs.extend(glue.attrs);
            items.extend(glue.items);
        }
    }
    diags.finish()?;

    let mut tokens = attrs;
    tokens.extend(item.to_token_stream());
    tokens.extend(items);
    Ok(tokens)
}
