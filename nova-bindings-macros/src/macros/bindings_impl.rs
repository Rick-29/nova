use syn::parse::{Parse, ParseStream};
use syn::{DeriveInput, Ident, Item, ItemImpl, Path, braced};
use zyn::{ToTokens, TokenStream};

use crate::backends::factory;
use crate::codegen::carrier::carrier_call;
use crate::codegen::markers::marker_impl;
use crate::codegen::naming::sibling;
use crate::ir::{BackendKind, IndicatorData, MethodData, MethodKind};
use crate::macros::derive_bindings::validate_options;
use crate::parser::attribute_parser::{ImplArgs, parse_args, parse_impl_args};
use crate::parser::impl_parser::{claimed_methods, parse_impl, strip_method_attrs, target_path};
use crate::parser::struct_parser::parse_struct;
use crate::validation::diagnostics::{Diagnostics, with_help};
use crate::validation::signatures::validate_method;

/// `#[bindings]` on an impl block: re-emits the block unchanged (minus
/// method-level `#[bindings(...)]` markers) and hands it to the indicator's
/// carrier macro for binding generation.
pub fn expand_attribute(args: TokenStream, item: TokenStream) -> TokenStream {
    let item: Item = match syn::parse2(item) {
        Ok(item) => item,
        Err(error) => return error.to_compile_error(),
    };
    let Item::Impl(item) = item else {
        let mut tokens = with_help(
            zyn::Span::call_site(),
            "`#[bindings]` goes on impl blocks",
            "on the indicator struct, use `#[derive(Bindings)]` with `#[bindings(error = MyError)]`",
        )
        .to_compile_error();
        tokens.extend(item.to_token_stream());
        return tokens;
    };

    let mut tokens = strip_method_attrs(&item).to_token_stream();
    let target = parse_args(args.clone())
        .and_then(|parsed| parse_impl_args(&parsed))
        .and_then(|impl_args| target_path(&item, &impl_args));
    match target {
        Ok(target) => tokens.extend(carrier_call(&target, args, item.to_token_stream())),
        Err(error) => tokens.extend(error.to_compile_error()),
    }
    tokens
}

/// Input of the hidden `__bindings_impl!` macro, assembled by the carrier.
struct CarriedInput {
    indicator: DeriveInput,
    target: Path,
    args: TokenStream,
    item: ItemImpl,
}

fn section<T>(
    input: ParseStream,
    name: &str,
    parse: impl FnOnce(ParseStream) -> syn::Result<T>,
) -> syn::Result<T> {
    let ident: Ident = input.parse()?;
    if ident != name {
        return Err(syn::Error::new(ident.span(), format!("expected `{name}`")));
    }
    let content;
    braced!(content in input);
    parse(&content)
}

impl Parse for CarriedInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            indicator: section(input, "indicator", |content| content.parse())?,
            target: section(input, "target", |content| content.parse())?,
            args: section(input, "args", |content| content.parse())?,
            item: section(input, "item", |content| content.parse())?,
        })
    }
}

/// `__bindings_impl!`: generates the bindings for one impl block, with the
/// struct declaration supplied by the carrier.
pub fn expand_carried(input: TokenStream) -> TokenStream {
    let input: CarriedInput = match syn::parse2(input) {
        Ok(input) => input,
        Err(error) => return error.to_compile_error(),
    };
    // Struct-level errors were already reported by the derive.
    let Ok(mut data) = parse_struct(&input.indicator) else {
        return TokenStream::new();
    };
    if validate_options(&data).is_err() {
        return TokenStream::new();
    }
    let Ok(impl_args) = parse_args(input.args.clone()).and_then(|args| parse_impl_args(&args))
    else {
        // Reported by the attribute expansion.
        return TokenStream::new();
    };
    match generate(&mut data, &input, &impl_args) {
        Ok(tokens) => tokens,
        Err(error) => {
            let mut tokens = error.to_compile_error();
            tokens.extend(claimed_markers(&data, &input, &impl_args));
            tokens
        }
    }
}

/// Markers for the methods a failing block meant to provide, so the error
/// is not followed by a "has no binding" error for each of them.
fn claimed_markers(
    data: &IndicatorData,
    input: &CarriedInput,
    impl_args: &ImplArgs,
) -> TokenStream {
    let mut tokens = TokenStream::new();
    for kind in claimed_methods(&input.item, impl_args) {
        for backend in &data.config.backends {
            let provides = if impl_args.method.is_some() {
                match &impl_args.backends {
                    Some((backends, _)) => backends.contains(backend),
                    None => data.config.is_overridden(kind, *backend),
                }
            } else {
                !data.config.is_overridden(kind, *backend)
            };
            if provides && kind != MethodKind::Custom {
                tokens.extend(marker_impl(data, kind, *backend, &input.target));
            }
        }
    }
    tokens
}

/// `true` when `backend` gets a binding for `method`: overrides apply to
/// their own backends, and a default is skipped where an override is declared.
pub fn binds(data: &IndicatorData, method: &MethodData, backend: BackendKind) -> bool {
    if method.is_override {
        method.override_backends.contains(&backend)
    } else {
        !data.config.is_overridden(method.kind, backend)
    }
}

fn generate(
    data: &mut IndicatorData,
    input: &CarriedInput,
    impl_args: &ImplArgs,
) -> syn::Result<TokenStream> {
    parse_impl(&input.item, impl_args, data)?;

    let builders = factory::selected(&data.config.backends);
    let mut diags = Diagnostics::new();
    for method in data.methods() {
        if diags.take(validate_method(method)).is_none() {
            continue;
        }
        for builder in &builders {
            if binds(data, method, builder.kind()) {
                diags.take(builder.validate_method(data, method));
            }
        }
    }
    diags.finish()?;

    let mut tokens = TokenStream::new();
    for builder in builders {
        let methods: Vec<&MethodData> = data
            .methods()
            .filter(|method| binds(data, method, builder.kind()))
            .collect();
        if methods.is_empty() {
            continue;
        }
        let wrapper_ident = builder.wrapper_ident(data);
        let wrapper = Path::from(wrapper_ident.clone());
        let mut block = builder.build_methods(data, &wrapper, &methods)?;
        for method in methods {
            block.extend(marker_impl(
                data,
                method.kind,
                builder.kind(),
                &input.target,
            ));
        }
        if input.target.segments.len() > 1 {
            // Frameworks such as UniFFI and wasm-bindgen need a plain
            // identifier as the impl self type, so bring the wrapper into a
            // block scope. Other paths keep resolving as in this module.
            let wrapper_path = sibling(&input.target, wrapper_ident);
            let feature = builder.feature();
            block = template! {
                const _: () = {
                    #[cfg(feature = {{ feature }})]
                    use {{ wrapper_path }};
                    {{ block }}
                };
            };
        }
        tokens.extend(block);
    }
    Ok(tokens)
}
