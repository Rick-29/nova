use syn::{Ident, LitStr, Type};
use zyn::Span;

use super::MethodKind;

/// A binding backend the macros know how to generate code for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BackendKind {
    Python,
    UniFfi,
    Wasm,
    Napi,
}

impl BackendKind {
    /// Every implemented backend, in generation order.
    pub const ALL: [BackendKind; 4] = [
        BackendKind::Python,
        BackendKind::UniFfi,
        BackendKind::Wasm,
        BackendKind::Napi,
    ];

    /// The name used in `backends(...)` and as the backend-options key.
    pub fn name(self) -> &'static str {
        match self {
            BackendKind::Python => "python",
            BackendKind::UniFfi => "uniffi",
            BackendKind::Wasm => "wasm",
            BackendKind::Napi => "napi",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub fn names() -> Vec<&'static str> {
        Self::ALL.iter().map(|kind| kind.name()).collect()
    }
}

/// Value of a backend option such as `python(update = both)`.
///
/// Options are normalized generically here; each backend decides which keys
/// and values it accepts.
#[derive(Clone, Debug)]
pub enum OptionValue {
    /// `key` with no value.
    Flag,
    /// `key = ident`.
    Ident(Ident),
    /// `key = "text"`.
    Str(LitStr),
}

#[derive(Clone, Debug)]
pub struct BackendOption {
    pub key: Ident,
    pub value: OptionValue,
}

/// Options given to one backend, e.g. `python(module = "nova_ta")`.
#[derive(Clone, Debug)]
pub struct BackendOptions {
    pub kind: BackendKind,
    pub span: Span,
    pub entries: Vec<BackendOption>,
}

/// A struct-level declaration that a standard method is replaced by a custom
/// adapter for some backends: `overrides(update(python))`.
#[derive(Clone, Debug)]
pub struct OverrideDecl {
    pub method: MethodKind,
    pub backends: Vec<BackendKind>,
}

/// Shared binding configuration from `#[bindings(...)]` on the indicator struct.
#[derive(Clone)]
pub struct BindingConfig {
    /// `error = Type`: the error type exported to foreign languages.
    pub error_type: Type,
    /// Backends to generate. Defaults to every implemented backend.
    pub backends: Vec<BackendKind>,
    /// `name = "..."`: foreign-language class name. Defaults to the Rust name.
    pub public_name: Option<LitStr>,
    pub overrides: Vec<OverrideDecl>,
    pub backend_options: Vec<BackendOptions>,
}

impl BindingConfig {
    /// `true` when `method` is declared as overridden for `backend`, so the
    /// default binding must not be generated.
    pub fn is_overridden(&self, method: MethodKind, backend: BackendKind) -> bool {
        self.overrides
            .iter()
            .any(|decl| decl.method == method && decl.backends.contains(&backend))
    }

    pub fn override_decl(&self, method: MethodKind) -> Option<&OverrideDecl> {
        self.overrides.iter().find(|decl| decl.method == method)
    }

    pub fn options(&self, kind: BackendKind) -> &[BackendOption] {
        self.backend_options
            .iter()
            .find(|options| options.kind == kind)
            .map(|options| options.entries.as_slice())
            .unwrap_or_default()
    }
}
