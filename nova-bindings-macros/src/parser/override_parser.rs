//! Override declarations.
//!
//! Replacing a standard binding takes two pieces, because no single macro
//! invocation sees every impl block:
//!
//! - the struct declares `overrides(update(python))`, so every expansion knows
//!   the default `update` must not be generated for Python;
//! - one impl block marked `#[bindings(method = update)]` supplies the adapter.

use syn::spanned::Spanned;
use zyn::Arg;
use zyn::Span;

use crate::ir::{BackendKind, MethodKind, OverrideDecl};
use crate::parser::attribute_parser::{arg_span, expect_ident, parse_backend_name};
use crate::validation::diagnostics::{Diagnostics, unknown_name, with_help};

fn parse_standard_method(ident: &syn::Ident) -> syn::Result<MethodKind> {
    MethodKind::from_standard_name(&ident.to_string()).ok_or_else(|| {
        unknown_name(
            ident.span(),
            "standard method",
            &ident.to_string(),
            &MethodKind::standard_names(),
        )
    })
}

/// `method = update` on an impl block.
pub fn parse_override_method(arg: &Arg) -> syn::Result<(MethodKind, Span)> {
    let ident = expect_ident(arg)?;
    Ok((parse_standard_method(&ident)?, ident.span()))
}

/// `overrides(update, new(python))` on the struct. A bare method name applies
/// to every selected backend.
pub fn parse_override_decls(arg: &Arg, selected: &[BackendKind]) -> syn::Result<Vec<OverrideDecl>> {
    let Arg::List(_, list) = arg else {
        return Err(with_help(
            arg_span(arg),
            "`overrides` expects a list of standard methods",
            "write `overrides(update)` or `overrides(update(python))`",
        ));
    };

    let mut diags = Diagnostics::new();
    let mut decls: Vec<OverrideDecl> = Vec::new();
    for item in list {
        let (name, backends) = match item {
            Arg::Flag(name) => (name, selected.to_vec()),
            Arg::List(name, backend_args) => {
                let mut backends = Vec::new();
                for backend in backend_args {
                    let Arg::Flag(ident) = backend else {
                        diags.error(backend.span(), "expected a backend name such as `python`");
                        continue;
                    };
                    let Some(kind) = diags.take(parse_backend_name(ident)) else {
                        continue;
                    };
                    if !selected.contains(&kind) {
                        diags.error(
                            ident.span(),
                            format!("cannot override for backend `{ident}`, which is not selected"),
                        );
                    } else if backends.contains(&kind) {
                        diags.error(ident.span(), format!("backend `{ident}` is listed twice"));
                    } else {
                        backends.push(kind);
                    }
                }
                (name, backends)
            }
            _ => {
                diags.error(item.span(), "expected `method` or `method(backend, ...)`");
                continue;
            }
        };
        let Some(method) = diags.take(parse_standard_method(name)) else {
            continue;
        };
        if decls.iter().any(|decl| decl.method == method) {
            diags.error(
                name.span(),
                format!("override for `{name}` is declared twice"),
            );
            continue;
        }
        decls.push(OverrideDecl { method, backends });
    }
    diags.finish_with(decls)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decls(tokens: &str, selected: &[BackendKind]) -> syn::Result<Vec<OverrideDecl>> {
        let arg: Arg = syn::parse_str(tokens).unwrap();
        parse_override_decls(&arg, selected)
    }

    #[test]
    fn bare_method_applies_to_selected_backends() {
        let parsed = decls(
            "overrides(update)",
            &[BackendKind::Python, BackendKind::Wasm],
        )
        .unwrap();
        assert_eq!(parsed[0].method, MethodKind::Update);
        assert_eq!(parsed[0].backends, [BackendKind::Python, BackendKind::Wasm]);
    }

    #[test]
    fn per_backend_overrides() {
        let parsed = decls("overrides(update(python), new(uniffi))", &BackendKind::ALL).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].backends, [BackendKind::Python]);
        assert_eq!(parsed[1].method, MethodKind::Constructor);
        assert_eq!(parsed[1].backends, [BackendKind::UniFfi]);
    }

    #[test]
    fn rejects_unknown_methods_and_duplicates() {
        let error = decls("overrides(updte, update, update)", &BackendKind::ALL).unwrap_err();
        let messages: Vec<String> = error.into_iter().map(|e| e.to_string()).collect();
        assert!(messages[0].contains("did you mean `update`?"));
        assert!(messages[1].contains("declared twice"));
    }

    #[test]
    fn rejects_unselected_backends() {
        let error = decls("overrides(update(wasm))", &[BackendKind::Python]).unwrap_err();
        assert!(error.to_string().contains("not selected"));
    }

    #[test]
    fn override_method_name() {
        let arg: Arg = syn::parse_str("method = ready").unwrap();
        assert_eq!(parse_override_method(&arg).unwrap().0, MethodKind::Ready);
        let arg: Arg = syn::parse_str("method = calculate").unwrap();
        assert!(parse_override_method(&arg).is_err());
    }
}
