//! End-to-end expansion tests: the generated code parses and contains the
//! expected backend items.

use zyn::TokenStream;

use super::{bindings_error, bindings_impl, derive_bindings};

const STRUCT: &str = r#"
    /// Correlation Trend Indicator.
    #[bindings(error = NovaError, overrides(update(python)), python(update = both))]
    pub struct Cti<T: Num> { x: Window<T> }
"#;

fn tokens(source: &str) -> TokenStream {
    source.parse().unwrap()
}

/// Compact text for substring assertions.
fn text(tokens: &TokenStream) -> String {
    tokens.to_string().replace(' ', "")
}

fn carried(args: &str, item: &str) -> TokenStream {
    tokens(&format!(
        "indicator {{ {STRUCT} }} target {{ Cti }} args {{ {args} }} item {{ {item} }}"
    ))
}

fn assert_parses(tokens: &TokenStream) {
    syn::parse2::<syn::File>(tokens.clone()).unwrap_or_else(|error| panic!("{error}\n{tokens}"));
}

#[test]
fn derive_generates_feature_gated_wrappers() {
    let output = derive_bindings::expand(tokens(STRUCT));
    assert_parses(&output);
    let code = text(&output);
    for expected in [
        r#"#[cfg(feature="python")]#[cfg_attr(feature="python-stubs",::pyo3_stub_gen::derive::gen_stub_pyclass)]#[::pyo3::pyclass(name="Cti")]"#,
        "pubstructPyCti{pub(crate)inner:Cti<f64,>,}",
        r#"#[cfg(feature="uniffi")]#[derive(::uniffi::Object)]#[uniffi(name="Cti")]"#,
        "pubstructUniffiCti{pub(crate)inner:::std::sync::Mutex<Cti<f64,>>,}",
        r#"#[cfg(feature="wasm")]#[::wasm_bindgen::prelude::wasm_bindgen(js_name=Cti)]"#,
        r#"#[cfg(feature="napi")]#[::napi_derive::napi(js_name="Cti")]"#,
        "macro_rules!__nova_bindings_Cti",
        "pub(crate)use__nova_bindings_Cti;",
        "pub(crate)trait__NovaBindingsCtiUpdatePython{}",
        "__nova_bindings_assert::<()>();",
        "#[doc=\"CorrelationTrendIndicator.\"]",
    ] {
        assert!(code.contains(expected), "missing `{expected}` in\n{output}");
    }
}

#[test]
fn backend_selection_limits_generated_items() {
    let output = derive_bindings::expand(tokens(
        "#[bindings(error = E, backends(uniffi))] pub struct Cti<T> { x: T }",
    ));
    let code = text(&output);
    assert!(code.contains("UniffiCti"));
    for absent in [
        "PyCti",
        "WasmCti",
        "NapiCti",
        "pyo3",
        "wasm_bindgen",
        "napi_derive",
    ] {
        assert!(!code.contains(absent), "unexpected `{absent}`");
    }
}

#[test]
fn invalid_struct_still_defines_a_silent_carrier() {
    let output = derive_bindings::expand(tokens("#[bindings(backends(python))] pub struct Cti;"));
    let code = text(&output);
    assert!(code.contains("compile_error!"));
    assert!(code.contains("macro_rules!__nova_bindings_Cti{($($tokens:tt)*)=>{};}"));
}

#[test]
fn impl_attribute_keeps_the_block_and_calls_the_carrier() {
    let output = bindings_impl::expand_attribute(
        TokenStream::new(),
        tokens(
            "impl<T: Num> Cti<T> { #[bindings(export)] pub fn len(&self) -> usize { 0 } pub fn new() -> Self { todo!() } }",
        ),
    );
    let code = text(&output);
    assert!(
        code.starts_with("impl<T:Num>Cti<T>{pubfnlen(&self)"),
        "{output}"
    );
    assert!(code.contains("__nova_bindings_Cti!{target{Cti}args{}item{"));
}

#[test]
fn constructor_bindings_delegate_to_the_original() {
    let output = bindings_impl::expand_carried(carried(
        "",
        "impl<T: Num> Cti<T> { /// Creates it.\n pub fn new(period: usize) -> NovaResult<Self> { todo!() } }",
    ));
    assert_parses(&output);
    let code = text(&output);
    for expected in [
        // Python: fallible constructor with the source parameter names.
        "#[new]#[pyo3(signature=(period,))]pubfnnew(period:usize,)->::pyo3::PyResult<Self>",
        "match<Cti<f64>>::new(period,)",
        "::pyo3::PyErr::from(<NovaErroras::core::convert::From<_>>::from(__error))",
        // UniFFI: `usize` crosses as `u64`, errors use the configured type.
        "#[uniffi::constructor]pubfnnew(period:u64,)->::core::result::Result<::std::sync::Arc<Self>,NovaError>",
        "<usizeas::core::convert::TryFrom<u64>>::try_from(period)",
        // wasm / napi.
        "#[wasm_bindgen(constructor)]pubfnnew(period:usize,)->::core::result::Result<Self,::wasm_bindgen::JsError>",
        "#[napi(constructor)]pubfnnew(period:u32,)->::napi::Result<Self>",
        "(period)asusize",
        // Completeness markers for every backend.
        "impl__NovaBindingsCtiNewPythonfor(){}",
        "impl__NovaBindingsCtiNewNapifor(){}",
        "#[doc=\"Createsit.\"]",
    ] {
        assert!(code.contains(expected), "missing `{expected}` in\n{output}");
    }
}

#[test]
fn trait_bindings_skip_overridden_defaults() {
    let output = bindings_impl::expand_carried(carried(
        "",
        "impl<T: Num> Indicator for Cti<T> {
            type Input = T; type Output = T; type Error = NovaError;
            fn update(&mut self, input: Self::Input) -> NovaResult<Self::Output> { todo!() }
            fn reset(&mut self) {}
            fn ready(&self) -> bool { true }
        }",
    ));
    assert_parses(&output);
    let code = text(&output);
    assert!(
        code.contains("<Cti<f64>asIndicator>::update(&mut*__inner"),
        "UniFFI locks the mutex"
    );
    assert!(code.contains("<Cti<f64>asIndicator>::ready(&self.inner,)"));
    assert!(code.contains("impl__NovaBindingsCtiUpdateUniffifor(){}"));
    // `update` is overridden for Python: only reset and ready are generated there.
    assert!(!code.contains("impl__NovaBindingsCtiUpdatePythonfor(){}"));
    assert!(!code.contains("__call__"));
    assert!(code.contains("impl__NovaBindingsCtiResetPythonfor(){}"));
}

#[test]
fn override_blocks_bind_the_adapter() {
    let output = bindings_impl::expand_carried(carried(
        "method = update",
        "impl Cti<f64> { pub fn update_batch(&mut self, values: Vec<f64>) -> NovaResult<Vec<f64>> { todo!() } }",
    ));
    assert_parses(&output);
    let code = text(&output);
    assert!(code.contains(
        "pubfnupdate(&mutself,values:::std::vec::Vec<f64>,)->::pyo3::PyResult<::std::vec::Vec<f64>>"
    ));
    assert!(
        code.contains("pubfn__call__(&mutself,values"),
        "python(update = both)"
    );
    assert!(code.contains("<Cti<f64>>::update_batch(&mutself.inner,values,)"));
    assert!(code.contains("impl__NovaBindingsCtiUpdatePythonfor(){}"));
    assert!(!code.contains("Uniffi"), "the override only targets Python");
}

#[test]
fn failing_blocks_still_claim_their_markers() {
    let output = bindings_impl::expand_carried(carried(
        "",
        "impl<T> Cti<T> { pub fn new(name: &str) -> Self { todo!() } }",
    ));
    let code = text(&output);
    assert!(code.contains("compile_error!"));
    assert!(code.contains("impl__NovaBindingsCtiNewPythonfor(){}"));
    assert!(!code.contains("pyo3::pymethods"));
}

#[test]
fn bindings_error_adds_backend_glue() {
    let output = bindings_error::expand(
        tokens("python(module = \"nova_ta\")"),
        tokens("/// Nova error.\n#[derive(Debug)] pub enum NovaError { InvalidPeriod }"),
    );
    assert_parses(&output);
    let code = text(&output);
    assert!(
        code.starts_with(
            r#"#[cfg_attr(feature="uniffi",derive(::uniffi::Error),uniffi(flat_error))]"#
        ),
        "{output}"
    );
    assert!(code.contains("::pyo3::create_exception!(nova_ta,NovaError,::pyo3::exceptions::PyValueError,\"Novaerror.\")"));
    assert!(code.contains("pubuse__nova_bindings_python_NovaError::NovaErrorasPyNovaError;"));
    assert!(code.contains("impl::core::convert::From<NovaError>for::pyo3::PyErr"));
    assert!(code.contains("__nova_bindings_assert_display::<NovaError>()"));
}

#[test]
fn bindings_error_rejects_uniffi_structs() {
    let output = bindings_error::expand(TokenStream::new(), tokens("pub struct NovaError;"));
    assert!(text(&output).contains("UniFFIerrorsmustbeenums"));
    let output =
        bindings_error::expand(tokens("backends(python)"), tokens("pub struct NovaError;"));
    assert!(!text(&output).contains("compile_error"));
}
