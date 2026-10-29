use std::collections::HashMap;

use syn::spanned::Spanned;
use syn::{GenericParam, Ident, ImplItem, ImplItemFn, ItemImpl, Path, Type};

use crate::ir::{CallTarget, IndicatorData, MethodData, MethodKind};
use crate::parser::attribute_parser::{ImplArgs, MethodArgs, parse_method_args};
use crate::parser::docs_parser::parse_docs;
use crate::parser::signature_parser::{TypeContext, parse_signature};
use crate::validation::diagnostics::{Diagnostics, with_help};

fn is_bindings_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("bindings")
}

/// Name of the indicator an impl block belongs to: `target = ...` when given,
/// otherwise the last segment of the self type.
pub fn target_path(item: &ItemImpl, args: &ImplArgs) -> syn::Result<Path> {
    if let Some(target) = &args.target {
        return Ok(target.clone());
    }
    match &*item.self_ty {
        Type::Path(path) if path.qself.is_none() => {
            let mut path = path.path.clone();
            if let Some(last) = path.segments.last_mut() {
                last.arguments = syn::PathArguments::None;
            }
            Ok(path)
        }
        other => Err(with_help(
            other.span(),
            "cannot tell which indicator this impl block belongs to",
            "name it explicitly with `#[bindings(target = MyIndicator)]`",
        )),
    }
}

/// The impl block with method-level `#[bindings(...)]` attributes removed, so
/// it compiles as ordinary Rust.
pub fn strip_method_attrs(item: &ItemImpl) -> ItemImpl {
    let mut item = item.clone();
    for impl_item in &mut item.items {
        if let ImplItem::Fn(function) = impl_item {
            function.attrs.retain(|attr| !is_bindings_attr(attr));
        }
    }
    item
}

/// Standard methods an impl block provides, judged by name alone. Used to
/// keep a failing block from also reporting its methods as missing.
pub fn claimed_methods(item: &ItemImpl, args: &ImplArgs) -> Vec<MethodKind> {
    if let Some((kind, _)) = args.method {
        return vec![kind];
    }
    let names = item.items.iter().filter_map(|impl_item| match impl_item {
        ImplItem::Fn(function) => Some((function.sig.ident.to_string(), function)),
        _ => None,
    });
    if item.trait_.is_some() {
        names
            .filter_map(|(name, _)| match name.as_str() {
                "update" => Some(MethodKind::Update),
                "reset" => Some(MethodKind::Reset),
                "ready" => Some(MethodKind::Ready),
                _ => None,
            })
            .collect()
    } else {
        let has_constructor = names.into_iter().any(|(name, function)| {
            name == "new"
                || function.attrs.iter().any(|attr| {
                    is_bindings_attr(attr)
                        && parse_method_args(attr).is_ok_and(|args| args.constructor)
                })
        });
        if has_constructor {
            vec![MethodKind::Constructor]
        } else {
            Vec::new()
        }
    }
}

/// Records the bindable methods of `item` into `indicator`.
pub fn parse_impl(
    item: &ItemImpl,
    args: &ImplArgs,
    indicator: &mut IndicatorData,
) -> syn::Result<()> {
    let mut diags = Diagnostics::new();

    if args.target.is_none()
        && let Ok(path) = target_path(item, args)
        && path.segments.last().map(|seg| &seg.ident) != Some(&indicator.rust_name)
    {
        diags.error_spanned(
            &item.self_ty,
            format!("this impl block is not for `{}`", indicator.rust_name),
        );
    }

    let mut type_params = Vec::new();
    for param in &item.generics.params {
        match param {
            GenericParam::Type(param) => type_params.push(param.ident.clone()),
            other => diags.error_spanned(
                other,
                "only type parameters are supported on bound impl blocks; they are instantiated with the backend's numeric type",
            ),
        }
    }

    let assoc_types: HashMap<String, Type> = item
        .items
        .iter()
        .filter_map(|impl_item| match impl_item {
            ImplItem::Type(assoc) => Some((assoc.ident.to_string(), assoc.ty.clone())),
            _ => None,
        })
        .collect();
    let ctx = TypeContext {
        struct_name: &indicator.rust_name,
        type_params,
        assoc_types,
    };
    let call = CallTarget {
        self_ty: (*item.self_ty).clone(),
        impl_generics: item.generics.clone(),
        trait_path: item.trait_.as_ref().map(|(_, path, _)| path.clone()),
    };

    let functions: Vec<&ImplItemFn> = item
        .items
        .iter()
        .filter_map(|impl_item| match impl_item {
            ImplItem::Fn(function) => Some(function),
            _ => None,
        })
        .collect();

    let mut methods = Vec::new();
    if let Some((kind, span)) = args.method {
        if item.trait_.is_some() {
            diags.error(
                span,
                "overrides must be inherent impl blocks, not trait impls",
            );
        }
        if let Some(method) = diags.take(override_method(
            &functions, kind, span, args, indicator, &ctx, &call,
        )) {
            methods.push(method);
        }
    } else if item.trait_.is_some() {
        for function in functions {
            for attr in function.attrs.iter().filter(|attr| is_bindings_attr(attr)) {
                diags.error(
                    attr.span(),
                    "method-level `#[bindings(...)]` is only allowed in inherent impl blocks",
                );
            }
            let kind = match function.sig.ident.to_string().as_str() {
                "update" => MethodKind::Update,
                "reset" => MethodKind::Reset,
                "ready" => MethodKind::Ready,
                _ => continue,
            };
            let binding_name = function.sig.ident.clone();
            if let Some(method) =
                diags.take(build_method(function, kind, binding_name, &ctx, &call))
            {
                methods.push(method);
            }
        }
    } else {
        methods.extend(inherent_methods(&functions, &ctx, &call, &mut diags));
    }

    for method in methods {
        match indicator.slot(method.kind) {
            Some(slot) => {
                if slot.is_some() {
                    diags.error(
                        method.span,
                        format!("`{}` is bound twice in this block", method.binding_name),
                    );
                }
                *slot = Some(method);
            }
            None => indicator.custom.push(method),
        }
    }

    diags.finish()
}

fn inherent_methods(
    functions: &[&ImplItemFn],
    ctx: &TypeContext<'_>,
    call: &CallTarget,
    diags: &mut Diagnostics,
) -> Vec<MethodData> {
    let mut parsed: Vec<(&ImplItemFn, MethodArgs)> = Vec::new();
    for function in functions {
        let mut method_args = MethodArgs::default();
        let mut attrs = function.attrs.iter().filter(|attr| is_bindings_attr(attr));
        if let Some(attr) = attrs.next()
            && let Some(args) = diags.take(parse_method_args(attr))
        {
            method_args = args;
        }
        for extra in attrs {
            diags.error(
                extra.span(),
                "only one `#[bindings(...)]` attribute is allowed per method",
            );
        }
        parsed.push((function, method_args));
    }

    let marked: Vec<&ImplItemFn> = parsed
        .iter()
        .filter(|(_, args)| args.constructor)
        .map(|(function, _)| *function)
        .collect();
    for extra in marked.iter().skip(1) {
        diags.error(
            extra.sig.ident.span(),
            "only one function per indicator can be the binding constructor",
        );
    }
    let constructor = marked.first().copied().or_else(|| {
        functions
            .iter()
            .copied()
            .find(|function| function.sig.ident == "new")
    });

    let mut methods = Vec::new();
    for (function, args) in &parsed {
        let is_constructor = constructor.is_some_and(|ctor| std::ptr::eq(ctor, *function));
        let (kind, binding_name) = if is_constructor {
            if let Some(name) = &args.name {
                diags.error(name.span(), "`name` has no effect on the constructor");
            }
            (
                MethodKind::Constructor,
                Ident::new("new", function.sig.ident.span()),
            )
        } else if args.export {
            let binding_name = match &args.name {
                Some(name) => Ident::new(&name.value(), name.span()),
                None => function.sig.ident.clone(),
            };
            if MethodKind::from_standard_name(&binding_name.to_string()).is_some() {
                diags.push(with_help(
                    binding_name.span(),
                    format!("`{binding_name}` is a standard binding method and cannot be exported as a custom method"),
                    format!("declare `overrides({binding_name})` on the struct and use `#[bindings(method = {binding_name})]` to replace it"),
                ));
                continue;
            }
            (MethodKind::Custom, binding_name)
        } else {
            continue;
        };
        if let Some(method) = diags.take(build_method(function, kind, binding_name, ctx, call)) {
            methods.push(method);
        }
    }
    methods
}

fn override_method(
    functions: &[&ImplItemFn],
    kind: MethodKind,
    span: zyn::Span,
    args: &ImplArgs,
    indicator: &IndicatorData,
    ctx: &TypeContext<'_>,
    call: &CallTarget,
) -> syn::Result<MethodData> {
    let mut diags = Diagnostics::new();
    let name = kind
        .standard_name()
        .expect("overrides target standard methods");

    let [function] = functions else {
        return Err(with_help(
            span,
            format!(
                "an override block must contain exactly one function, found {}",
                functions.len()
            ),
            "put helper functions in a separate impl block",
        ));
    };
    for attr in function.attrs.iter().filter(|attr| is_bindings_attr(attr)) {
        diags.error(
            attr.span(),
            "method-level `#[bindings(...)]` is not allowed in an override block",
        );
    }
    if kind != MethodKind::Constructor
        && MethodKind::from_standard_name(&function.sig.ident.to_string()).is_some()
    {
        diags.push(with_help(
            function.sig.ident.span(),
            format!(
                "an inherent `{}` would shadow the trait method for Rust callers",
                function.sig.ident
            ),
            "give the adapter a different Rust name, e.g. `update_batch`",
        ));
    }

    let backends = match indicator.config.override_decl(kind) {
        None => {
            diags.push(with_help(
                span,
                format!("`{}` does not declare an override for `{name}`", indicator.rust_name),
                format!("add `overrides({name})` to `#[bindings(...)]` on `{}` so the default binding is not generated", indicator.rust_name),
            ));
            Vec::new()
        }
        Some(decl) => match &args.backends {
            None => decl.backends.clone(),
            Some((backends, backends_span)) => {
                for backend in backends {
                    if !decl.backends.contains(backend) {
                        diags.push(with_help(
                            *backends_span,
                            format!(
                                "`{name}` is not declared as overridden for backend `{}`",
                                backend.name()
                            ),
                            format!(
                                "add it to `overrides({name}(...))` on `{}`",
                                indicator.rust_name
                            ),
                        ));
                    }
                }
                backends.clone()
            }
        },
    };

    let method = build_method(
        function,
        kind,
        Ident::new(name, function.sig.ident.span()),
        ctx,
        call,
    )?;
    diags.finish_with(MethodData {
        is_override: true,
        override_backends: backends,
        ..method
    })
}

fn build_method(
    function: &ImplItemFn,
    kind: MethodKind,
    binding_name: Ident,
    ctx: &TypeContext<'_>,
    call: &CallTarget,
) -> syn::Result<MethodData> {
    let parsed = parse_signature(&function.sig, ctx)?;
    Ok(MethodData {
        rust_name: function.sig.ident.clone(),
        binding_name,
        signature: function.sig.clone(),
        docs: parse_docs(&function.attrs),
        kind,
        is_override: false,
        override_backends: Vec::new(),
        receiver: parsed.receiver,
        args: parsed.args,
        output: parsed.output,
        call: call.clone(),
        span: function.sig.ident.span(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BackendKind, Receiver};
    use crate::parser::attribute_parser::{parse_args, parse_impl_args};
    use crate::parser::struct_parser::parse_struct;
    use syn::parse_quote;

    fn indicator(config: &str) -> IndicatorData {
        let attr: zyn::TokenStream = config.parse().unwrap();
        let input: syn::DeriveInput = parse_quote! {
            #[bindings(#attr)]
            pub struct Cti<T> { x: T }
        };
        parse_struct(&input).unwrap()
    }

    fn parse(indicator: &mut IndicatorData, args: &str, item: ItemImpl) -> syn::Result<()> {
        let args = parse_impl_args(&parse_args(args.parse().unwrap()).unwrap()).unwrap();
        parse_impl(&item, &args, indicator)
    }

    #[test]
    fn inherent_block_constructor_and_exports() {
        let mut data = indicator("error = E");
        parse(
            &mut data,
            "",
            parse_quote! {
                impl<T: Num> Cti<T> {
                    /// Creates it.
                    pub fn new(period: usize) -> NovaResult<Self> { todo!() }
                    #[bindings(export, name = "length")]
                    pub fn period(&self) -> usize { 0 }
                    pub fn helper(&self) -> &Window<T> { todo!() }
                }
            },
        )
        .unwrap();
        let ctor = data.constructor.as_ref().unwrap();
        assert_eq!(ctor.binding_name, "new");
        assert_eq!(ctor.args[0].name, "period");
        assert_eq!(ctor.docs.summary.as_deref(), Some("Creates it."));
        assert_eq!(data.custom.len(), 1);
        assert_eq!(data.custom[0].binding_name, "length");
        assert_eq!(data.custom[0].rust_name, "period");
    }

    #[test]
    fn explicit_constructor_marker() {
        let mut data = indicator("error = E");
        parse(
            &mut data,
            "",
            parse_quote! {
                impl<T> Cti<T> {
                    pub fn new() -> Self { todo!() }
                    #[bindings(constructor)]
                    pub fn with_period(period: usize) -> Self { todo!() }
                }
            },
        )
        .unwrap();
        let ctor = data.constructor.unwrap();
        assert_eq!(ctor.rust_name, "with_period");
        assert_eq!(ctor.binding_name, "new");
    }

    #[test]
    fn trait_block_standard_methods() {
        let mut data = indicator("error = E");
        parse(
            &mut data,
            "",
            parse_quote! {
                impl<T: Num> Indicator for Cti<T> {
                    type Input = T;
                    type Output = T;
                    type Error = NovaError;
                    /// Updates.
                    fn update(&mut self, input: Self::Input) -> NovaResult<Self::Output> { todo!() }
                    fn reset(&mut self) {}
                    fn ready(&self) -> bool { true }
                }
            },
        )
        .unwrap();
        let update = data.update.as_ref().unwrap();
        assert_eq!(update.receiver, Receiver::RefMut);
        assert!(update.args[0].ty.is_float_scalar());
        assert!(update.call.trait_path.is_some());
        assert!(data.reset.is_some() && data.ready.is_some());
        assert!(data.constructor.is_none());
    }

    #[test]
    fn override_block() {
        let mut data = indicator("error = E, overrides(update(python, uniffi))");
        parse(&mut data, "method = update, backends(python)", parse_quote! {
            impl Cti<f64> {
                pub fn update_batch(&mut self, values: Vec<f64>) -> NovaResult<Vec<f64>> { todo!() }
            }
        })
        .unwrap();
        let update = data.update.unwrap();
        assert!(update.is_override);
        assert_eq!(update.rust_name, "update_batch");
        assert_eq!(update.binding_name, "update");
        assert_eq!(update.override_backends, [BackendKind::Python]);
    }

    #[test]
    fn override_requires_declaration() {
        let mut data = indicator("error = E");
        let error = parse(
            &mut data,
            "method = update",
            parse_quote! {
                impl Cti<f64> { pub fn update_batch(&mut self, v: Vec<f64>) -> Vec<f64> { v } }
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("does not declare an override for `update`")
        );
    }

    #[test]
    fn override_rejects_shadowing_and_extra_functions() {
        let mut data = indicator("error = E, overrides(update)");
        let error = parse(
            &mut data,
            "method = update",
            parse_quote! {
                impl Cti<f64> { pub fn update(&mut self, v: f64) -> f64 { v } }
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("shadow"));

        let error = parse(
            &mut data,
            "method = update",
            parse_quote! {
                impl Cti<f64> { fn a(&mut self) {} fn b(&mut self) {} }
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("exactly one function"));
    }

    #[test]
    fn rejects_custom_methods_named_like_standard_ones() {
        let mut data = indicator("error = E");
        let error = parse(
            &mut data,
            "",
            parse_quote! {
                impl Cti<f64> { #[bindings(export)] pub fn reset(&mut self) {} }
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("standard binding method"));
    }

    #[test]
    fn rejects_blocks_for_other_types() {
        let mut data = indicator("error = E");
        let error = parse(
            &mut data,
            "",
            parse_quote!(impl Other { fn new() -> Self { todo!() } }),
        )
        .unwrap_err();
        assert!(error.to_string().contains("not for `Cti`"));
    }

    #[test]
    fn strips_method_attributes_and_infers_target() {
        let item: ItemImpl = parse_quote! {
            impl<T> crate::stats::Cti<T> { #[bindings(export)] #[inline] pub fn a(&self) {} }
        };
        let stripped = strip_method_attrs(&item);
        let ImplItem::Fn(function) = &stripped.items[0] else {
            unreachable!()
        };
        assert_eq!(function.attrs.len(), 1);
        let path = target_path(&item, &ImplArgs::default()).unwrap();
        assert_eq!(
            zyn::ToTokens::to_token_stream(&path).to_string(),
            "crate :: stats :: Cti"
        );
    }
}
