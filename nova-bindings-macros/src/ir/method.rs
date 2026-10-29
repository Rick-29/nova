use syn::{Generics, Ident, Path, Type};
use zyn::Span;

use super::{BackendKind, BindingType, Docs};

/// The role a method plays in the binding contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MethodKind {
    Constructor,
    Update,
    Reset,
    Ready,
    /// An extra method opted in with `#[bindings(export)]`.
    Custom,
}

impl MethodKind {
    /// The four operations of the standard binding contract.
    pub const STANDARD: [MethodKind; 4] = [
        MethodKind::Constructor,
        MethodKind::Update,
        MethodKind::Reset,
        MethodKind::Ready,
    ];

    /// The method name used by the standard contract (`new`, `update`, ...).
    pub fn standard_name(self) -> Option<&'static str> {
        match self {
            MethodKind::Constructor => Some("new"),
            MethodKind::Update => Some("update"),
            MethodKind::Reset => Some("reset"),
            MethodKind::Ready => Some("ready"),
            MethodKind::Custom => None,
        }
    }

    pub fn from_standard_name(name: &str) -> Option<Self> {
        Self::STANDARD
            .into_iter()
            .find(|kind| kind.standard_name() == Some(name))
    }

    pub fn standard_names() -> Vec<&'static str> {
        Self::STANDARD
            .iter()
            .filter_map(|kind| kind.standard_name())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receiver {
    /// Associated function (constructors).
    None,
    /// `&self`.
    Ref,
    /// `&mut self`.
    RefMut,
}

/// One method argument.
#[derive(Clone)]
pub struct ArgData {
    /// Binding name, taken from the source pattern (`mut` stripped).
    pub name: Ident,
    pub ty: BindingType,
    /// The type as written in the source.
    pub rust_ty: Type,
    pub span: Span,
}

/// What a method returns.
#[derive(Clone)]
pub struct ReturnData {
    /// The success type: the `T` of `Result<T, E>`, or the whole return type.
    pub ok: BindingType,
    /// `true` when the return type is a `Result` (or an alias ending in `Result`).
    pub fallible: bool,
    pub span: Span,
}

/// Where the generated wrapper delegates to.
#[derive(Clone)]
pub struct CallTarget {
    /// The impl self type as written, e.g. `Cti<T>`.
    pub self_ty: Type,
    /// Generics of the impl block, instantiated by each backend.
    pub impl_generics: Generics,
    /// The implemented trait for trait impl blocks, e.g. `Indicator`.
    pub trait_path: Option<Path>,
}

/// A method discovered in an annotated impl block.
#[derive(Clone)]
pub struct MethodData {
    /// Name of the Rust method that is called.
    pub rust_name: Ident,
    /// Name exposed to foreign languages.
    pub binding_name: Ident,
    pub signature: syn::Signature,
    pub docs: Docs,
    pub kind: MethodKind,
    /// `true` when this method replaces a standard binding (see `overrides`).
    pub is_override: bool,
    /// Backends an override applies to. Empty for non-override methods.
    pub override_backends: Vec<BackendKind>,
    pub receiver: Receiver,
    pub args: Vec<ArgData>,
    pub output: ReturnData,
    pub call: CallTarget,
    pub span: Span,
}
