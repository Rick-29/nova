//! Typed parsing of `#[bindings(...)]` and `#[bindings_error(...)]` arguments.
//!
//! Syntax is read with [`zyn::Args`]; this module turns the generic argument
//! tree into IR values with span-accurate diagnostics.

use syn::spanned::Spanned;
use syn::{Attribute, Expr, ExprLit, Ident, Lit, LitStr, Meta, Path, Type};
use zyn::ToTokens;
use zyn::{Arg, Args};
use zyn::{Span, TokenStream};

use crate::ir::{
    BackendKind, BackendOption, BackendOptions, BindingConfig, MethodKind, OptionValue,
};
use crate::parser::override_parser;
use crate::validation::diagnostics::{Diagnostics, WithContext, unknown_name, with_help};

/// Parses the arguments of an attribute; `#[bindings]` yields no arguments.
pub fn attr_args(attr: &Attribute) -> syn::Result<Args> {
    match &attr.meta {
        Meta::Path(_) => Ok(Args::new()),
        Meta::List(list) => syn::parse2(list.tokens.clone()),
        Meta::NameValue(meta) => Err(syn::Error::new_spanned(
            meta,
            "expected a parenthesized argument list, e.g. `#[bindings(error = MyError)]`",
        )),
    }
}

pub fn parse_args(tokens: TokenStream) -> syn::Result<Args> {
    syn::parse2(tokens)
}

/// Span of an argument's key, or of the whole argument for literals.
pub fn arg_span(arg: &Arg) -> Span {
    match arg.name() {
        Some(name) => name.span(),
        None => arg.span(),
    }
}

/// Reports unknown and repeated keys.
pub fn check_keys(args: &Args, known: &[&str], context: &str, diags: &mut Diagnostics) {
    let mut seen: Vec<String> = Vec::new();
    for arg in args {
        let Some(name) = arg.name() else {
            diags.error(arg.span(), format!("unexpected literal in {context}"));
            continue;
        };
        let key = name.to_string();
        if !known.contains(&key.as_str()) {
            diags.push(unknown_name(name.span(), "argument", &key, known).with_context(context));
        } else if seen.contains(&key) {
            diags.error(
                name.span(),
                format!("duplicate argument `{key}` in {context}"),
            );
        } else {
            seen.push(key);
        }
    }
}

fn expect_expr<'a>(arg: &'a Arg, example: &str) -> syn::Result<&'a Expr> {
    match arg {
        Arg::Expr(_, expr) => Ok(expr),
        _ => Err(with_help(
            arg_span(arg),
            format!("`{}` expects a value", key(arg)),
            format!("write `{example}`"),
        )),
    }
}

fn key(arg: &Arg) -> String {
    arg.name().map(ToString::to_string).unwrap_or_default()
}

/// `key = Type`.
pub fn expect_type(arg: &Arg) -> syn::Result<Type> {
    let expr = expect_expr(arg, &format!("{} = MyError", key(arg)))?;
    syn::parse2(expr.to_token_stream()).map_err(|_| {
        syn::Error::new_spanned(
            expr,
            format!("`{}` expects a type, e.g. `MyError`", key(arg)),
        )
    })
}

/// `key = path::to::Item`.
pub fn expect_path(arg: &Arg) -> syn::Result<Path> {
    match expect_expr(arg, &format!("{} = Name", key(arg)))? {
        Expr::Path(path) if path.qself.is_none() => Ok(path.path.clone()),
        other => Err(syn::Error::new_spanned(
            other,
            format!("`{}` expects a path, e.g. `Name`", key(arg)),
        )),
    }
}

/// `key = ident`.
pub fn expect_ident(arg: &Arg) -> syn::Result<Ident> {
    let path = expect_path(arg)?;
    path.get_ident().cloned().ok_or_else(|| {
        syn::Error::new_spanned(&path, format!("`{}` expects a single identifier", key(arg)))
    })
}

/// `key = "text"`.
pub fn expect_str(arg: &Arg) -> syn::Result<LitStr> {
    match expect_expr(arg, &format!("{} = \"...\"", key(arg)))? {
        Expr::Lit(ExprLit {
            lit: Lit::Str(text),
            ..
        }) => Ok(text.clone()),
        other => Err(syn::Error::new_spanned(
            other,
            format!("`{}` expects a string literal", key(arg)),
        )),
    }
}

/// A bare `key` flag.
pub fn expect_flag(arg: &Arg) -> syn::Result<()> {
    match arg {
        Arg::Flag(_) => Ok(()),
        _ => Err(syn::Error::new(
            arg.span(),
            format!("`{}` is a flag and takes no value", key(arg)),
        )),
    }
}

fn expect_list<'a>(arg: &'a Arg, example: &str) -> syn::Result<&'a Args> {
    match arg {
        Arg::List(_, args) => Ok(args),
        _ => Err(with_help(
            arg_span(arg),
            format!("`{}` expects a parenthesized list", key(arg)),
            format!("write `{example}`"),
        )),
    }
}

pub fn parse_backend_name(ident: &Ident) -> syn::Result<BackendKind> {
    BackendKind::from_name(&ident.to_string()).ok_or_else(|| {
        unknown_name(
            ident.span(),
            "backend",
            &ident.to_string(),
            &BackendKind::names(),
        )
    })
}

/// `backends(python, uniffi)`.
pub fn parse_backend_list(arg: &Arg) -> syn::Result<Vec<BackendKind>> {
    let list = expect_list(arg, "backends(python, uniffi)")?;
    let mut diags = Diagnostics::new();
    let mut backends = Vec::new();
    for item in list {
        let Arg::Flag(ident) = item else {
            diags.error(item.span(), "expected a backend name such as `python`");
            continue;
        };
        if let Some(kind) = diags.take(parse_backend_name(ident)) {
            if backends.contains(&kind) {
                diags.error(ident.span(), format!("backend `{ident}` is listed twice"));
            } else {
                backends.push(kind);
            }
        }
    }
    if list.is_empty() {
        diags.push(with_help(
            arg.span(),
            "`backends()` selects no backend",
            "list at least one backend, or remove `backends(...)` to select all of them",
        ));
    }
    diags.finish_with(backends)
}

/// `python(module = "nova_ta", update = both)`.
pub fn parse_backend_options(arg: &Arg, kind: BackendKind) -> syn::Result<BackendOptions> {
    let list = expect_list(arg, &format!("{}(option = value)", kind.name()))?;
    let mut diags = Diagnostics::new();
    let mut entries = Vec::new();
    for item in list {
        let value = match item {
            Arg::Flag(_) => Some(OptionValue::Flag),
            Arg::Expr(
                _,
                Expr::Lit(ExprLit {
                    lit: Lit::Str(text),
                    ..
                }),
            ) => Some(OptionValue::Str(text.clone())),
            Arg::Expr(_, Expr::Path(path)) if path.path.get_ident().is_some() => {
                path.path.get_ident().cloned().map(OptionValue::Ident)
            }
            _ => {
                diags.error(
                    item.span(),
                    "backend options take the form `key`, `key = ident` or `key = \"text\"`",
                );
                None
            }
        };
        if let (Some(key), Some(value)) = (item.name(), value) {
            if entries
                .iter()
                .any(|entry: &BackendOption| entry.key == *key)
            {
                diags.error(key.span(), format!("duplicate option `{key}`"));
            }
            entries.push(BackendOption {
                key: key.clone(),
                value,
            });
        }
    }
    diags.finish_with(BackendOptions {
        kind,
        span: arg_span(arg),
        entries,
    })
}

fn known_with_backends(base: &[&'static str]) -> Vec<&'static str> {
    base.iter().copied().chain(BackendKind::names()).collect()
}

/// Parses the struct-level `#[bindings(...)]` configuration.
pub fn parse_binding_config(args: &Args, attr_span: Span) -> syn::Result<BindingConfig> {
    let mut diags = Diagnostics::new();
    let known = known_with_backends(&["error", "backends", "name", "overrides"]);
    check_keys(args, &known, "`#[bindings(...)]`", &mut diags);

    // A misspelled `error` is already reported as unknown with a suggestion.
    let misspelled_error = args.iter().any(|arg| {
        arg.name().is_some_and(|name| {
            name != "error" && zyn::closest_match(&name.to_string(), &known) == Some("error")
        })
    });
    let error_type = match args.get("error") {
        Some(arg) => diags.take(expect_type(arg)),
        None if misspelled_error => None,
        None => {
            diags.push(with_help(
                attr_span,
                "missing required argument `error`",
                "name the error type exported to foreign languages, e.g. `#[bindings(error = NovaError)]`",
            ));
            None
        }
    };

    let backends = match args.get("backends") {
        Some(arg) => diags.take(parse_backend_list(arg)).unwrap_or_default(),
        None => BackendKind::ALL.to_vec(),
    };

    let public_name = args.get("name").and_then(|arg| diags.take(expect_str(arg)));
    if let Some(name) = &public_name
        && syn::parse_str::<Ident>(&name.value()).is_err()
    {
        diags.error(name.span(), "`name` must be a valid identifier");
    }

    let mut backend_options = Vec::new();
    for kind in BackendKind::ALL {
        let Some(arg) = args.get(kind.name()) else {
            continue;
        };
        if let Some(options) = diags.take(parse_backend_options(arg, kind)) {
            if !backends.contains(&kind) {
                diags.push(with_help(
                    options.span,
                    format!(
                        "options are given for backend `{}`, which is not selected",
                        kind.name()
                    ),
                    format!(
                        "add `{}` to `backends(...)` or remove these options",
                        kind.name()
                    ),
                ));
            }
            backend_options.push(options);
        }
    }

    let overrides = match args.get("overrides") {
        Some(arg) => diags
            .take(override_parser::parse_override_decls(arg, &backends))
            .unwrap_or_default(),
        None => Vec::new(),
    };

    let error_type = diags.finish_with(error_type)?;
    Ok(BindingConfig {
        error_type: error_type.expect("missing `error` is reported above"),
        backends,
        public_name,
        overrides,
        backend_options,
    })
}

/// Arguments of `#[bindings(...)]` on an impl block.
#[derive(Default)]
pub struct ImplArgs {
    /// `method = update`: the block is a custom adapter replacing that method.
    pub method: Option<(MethodKind, Span)>,
    /// `backends(...)`: restricts an override to these backends.
    pub backends: Option<(Vec<BackendKind>, Span)>,
    /// `target = path::Indicator`: the annotated indicator, when it cannot be
    /// inferred from the self type or lives in another module.
    pub target: Option<Path>,
}

pub fn parse_impl_args(args: &Args) -> syn::Result<ImplArgs> {
    let mut diags = Diagnostics::new();
    check_keys(
        args,
        &["method", "backends", "target"],
        "`#[bindings(...)]` on an impl block",
        &mut diags,
    );

    let method = args
        .get("method")
        .and_then(|arg| diags.take(override_parser::parse_override_method(arg)));
    let backends = args.get("backends").and_then(|arg| {
        diags
            .take(parse_backend_list(arg))
            .map(|list| (list, arg_span(arg)))
    });
    if let (Some(arg), None) = (args.get("backends"), &method) {
        diags.push(with_help(
            arg_span(arg),
            "`backends(...)` on an impl block only applies to overrides",
            "add `method = <name>`, or select backends on the struct's `#[bindings(...)]`",
        ));
    }
    let target = args
        .get("target")
        .and_then(|arg| diags.take(expect_path(arg)));

    diags.finish_with(ImplArgs {
        method,
        backends,
        target,
    })
}

/// Arguments of `#[bindings(...)]` on a method inside an inherent impl block.
#[derive(Default)]
pub struct MethodArgs {
    /// `export`: expose this method as an additional binding method.
    pub export: bool,
    /// `constructor`: use this function as the binding constructor.
    pub constructor: bool,
    /// `name = "..."`: foreign-language method name.
    pub name: Option<LitStr>,
}

pub fn parse_method_args(attr: &Attribute) -> syn::Result<MethodArgs> {
    let args = attr_args(attr)?;
    let mut diags = Diagnostics::new();
    check_keys(
        &args,
        &["export", "constructor", "name"],
        "method-level `#[bindings(...)]`",
        &mut diags,
    );

    let export = args
        .get("export")
        .is_some_and(|arg| diags.take(expect_flag(arg)).is_some());
    let constructor = args
        .get("constructor")
        .is_some_and(|arg| diags.take(expect_flag(arg)).is_some());
    let name = args.get("name").and_then(|arg| diags.take(expect_str(arg)));
    if let Some(name) = &name
        && syn::parse_str::<Ident>(&name.value()).is_err()
    {
        diags.error(name.span(), "`name` must be a valid identifier");
    }
    if export && constructor {
        diags.error(
            attr.span(),
            "a method cannot be both `export` and `constructor`",
        );
    }
    if args.is_empty() {
        diags.push(with_help(
            attr.span(),
            "method-level `#[bindings]` needs an argument",
            "use `#[bindings(export)]` or `#[bindings(constructor)]`",
        ));
    }

    diags.finish_with(MethodArgs {
        export,
        constructor,
        name,
    })
}

/// Arguments of `#[bindings_error(...)]`.
pub struct ErrorArgs {
    pub backends: Vec<BackendKind>,
    pub backend_options: Vec<BackendOptions>,
}

pub fn parse_error_args(args: &Args) -> syn::Result<ErrorArgs> {
    let mut diags = Diagnostics::new();
    let known = known_with_backends(&["backends"]);
    check_keys(args, &known, "`#[bindings_error(...)]`", &mut diags);

    let backends = match args.get("backends") {
        Some(arg) => diags.take(parse_backend_list(arg)).unwrap_or_default(),
        None => BackendKind::ALL.to_vec(),
    };
    let mut backend_options = Vec::new();
    for kind in BackendKind::ALL {
        if let Some(arg) = args.get(kind.name())
            && let Some(options) = diags.take(parse_backend_options(arg, kind))
        {
            if !backends.contains(&kind) {
                diags.error(
                    options.span,
                    format!(
                        "options are given for backend `{}`, which is not selected",
                        kind.name()
                    ),
                );
            }
            backend_options.push(options);
        }
    }

    diags.finish_with(ErrorArgs {
        backends,
        backend_options,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(tokens: &str) -> syn::Result<BindingConfig> {
        let args = parse_args(tokens.parse().unwrap())?;
        parse_binding_config(&args, Span::call_site())
    }

    fn messages(result: syn::Result<impl Sized>) -> Vec<String> {
        match result {
            Ok(_) => Vec::new(),
            Err(error) => error.into_iter().map(|e| e.to_string()).collect(),
        }
    }

    #[test]
    fn defaults_select_every_backend() {
        let config = config("error = NovaError").unwrap();
        assert_eq!(config.backends, BackendKind::ALL);
        assert_eq!(config.error_type.to_token_stream().to_string(), "NovaError");
        assert!(config.overrides.is_empty());
        assert!(config.public_name.is_none());
    }

    #[test]
    fn explicit_backend_selection() {
        let config = config("error = crate::error::NovaError, backends(uniffi, python)").unwrap();
        assert_eq!(config.backends, [BackendKind::UniFfi, BackendKind::Python]);
        assert_eq!(
            config.error_type.to_token_stream().to_string(),
            "crate :: error :: NovaError"
        );
    }

    #[test]
    fn error_type_is_required() {
        let errors = messages(config("backends(python)"));
        assert!(errors[0].starts_with("missing required argument `error`"));
    }

    #[test]
    fn unknown_backend_suggests_a_fix() {
        let errors = messages(config("error = E, backends(pyhton)"));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("unknown backend `pyhton`"));
        assert!(errors[0].contains("did you mean `python`?"));
    }

    #[test]
    fn unknown_and_duplicate_keys_are_reported_together() {
        let errors = messages(config("eror = F"));
        assert_eq!(
            errors.len(),
            1,
            "a misspelled `error` is not also reported missing: {errors:?}"
        );
        let errors = messages(config(
            "error = E, eror = F, error = G, backends(python, python)",
        ));
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors[0].contains("did you mean `error`?"));
        assert!(errors[1].contains("duplicate argument `error`"));
        assert!(errors[2].contains("listed twice"));
    }

    #[test]
    fn empty_backend_list_is_rejected() {
        let errors = messages(config("error = E, backends()"));
        assert!(errors[0].contains("selects no backend"));
    }

    #[test]
    fn backend_options_are_normalized() {
        let config =
            config("error = E, python(module = \"nova_ta\", update = both, flag)").unwrap();
        let options = config.options(BackendKind::Python);
        assert_eq!(options.len(), 3);
        assert!(matches!(&options[0].value, OptionValue::Str(s) if s.value() == "nova_ta"));
        assert!(matches!(&options[1].value, OptionValue::Ident(i) if i == "both"));
        assert!(matches!(options[2].value, OptionValue::Flag));
        assert!(config.options(BackendKind::UniFfi).is_empty());
    }

    #[test]
    fn options_for_unselected_backend_are_rejected() {
        let errors = messages(config("error = E, backends(uniffi), python(update = both)"));
        assert!(errors[0].contains("not selected"));
    }

    #[test]
    fn public_name_must_be_an_identifier() {
        assert_eq!(
            config("error = E, name = \"Ema\"")
                .unwrap()
                .public_name
                .unwrap()
                .value(),
            "Ema"
        );
        assert!(
            messages(config("error = E, name = \"not valid\""))[0].contains("valid identifier")
        );
    }

    #[test]
    fn impl_args() {
        let args = parse_args(
            "method = update, backends(python), target = super::Cti"
                .parse()
                .unwrap(),
        )
        .unwrap();
        let parsed = parse_impl_args(&args).unwrap();
        assert_eq!(parsed.method.unwrap().0, MethodKind::Update);
        assert_eq!(parsed.backends.unwrap().0, [BackendKind::Python]);
        assert_eq!(
            parsed.target.unwrap().to_token_stream().to_string(),
            "super :: Cti"
        );

        let args = parse_args("backends(python)".parse().unwrap()).unwrap();
        assert!(messages(parse_impl_args(&args))[0].contains("only applies to overrides"));
    }

    #[test]
    fn method_args() {
        let attr: Attribute = syn::parse_quote!(#[bindings(export, name = "period_length")]);
        let parsed = parse_method_args(&attr).unwrap();
        assert!(parsed.export && !parsed.constructor);
        assert_eq!(parsed.name.unwrap().value(), "period_length");

        let attr: Attribute = syn::parse_quote!(#[bindings(export, constructor)]);
        assert!(messages(parse_method_args(&attr))[0].contains("both"));

        let attr: Attribute = syn::parse_quote!(#[bindings(exprt)]);
        assert!(messages(parse_method_args(&attr))[0].contains("did you mean `export`?"));
    }

    #[test]
    fn error_args() {
        let args = parse_args(
            "backends(python), python(base = RuntimeError)"
                .parse()
                .unwrap(),
        )
        .unwrap();
        let parsed = parse_error_args(&args).unwrap();
        assert_eq!(parsed.backends, [BackendKind::Python]);
        assert_eq!(parsed.backend_options.len(), 1);
    }
}
