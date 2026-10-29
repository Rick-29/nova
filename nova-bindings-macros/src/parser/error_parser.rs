use syn::Item;

use crate::ir::ErrorData;
use crate::parser::attribute_parser::ErrorArgs;
use crate::parser::docs_parser::parse_docs;

/// Parses the item annotated with `#[bindings_error]`.
pub fn parse_error(item: &Item, args: ErrorArgs) -> syn::Result<ErrorData> {
    let (ident, vis, attrs, is_enum) = match item {
        Item::Enum(item) => (&item.ident, &item.vis, &item.attrs, true),
        Item::Struct(item) => (&item.ident, &item.vis, &item.attrs, false),
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "`#[bindings_error]` goes on the error enum or struct exported to foreign languages",
            ));
        }
    };
    let generic = match item {
        Item::Enum(item) => !item.generics.params.is_empty(),
        Item::Struct(item) => !item.generics.params.is_empty(),
        _ => false,
    };
    if generic {
        return Err(syn::Error::new_spanned(
            ident,
            "binding error types cannot be generic",
        ));
    }
    Ok(ErrorData {
        ident: ident.clone(),
        vis: vis.clone(),
        docs: parse_docs(attrs),
        is_enum,
        backends: args.backends,
        backend_options: args.backend_options,
        span: ident.span(),
    })
}
