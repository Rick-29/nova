use syn::{Ident, Visibility};
use zyn::Span;

use super::{BackendKind, BackendOption, BackendOptions, Docs};

/// The runtime error type annotated with `#[bindings_error]`.
#[derive(Clone)]
pub struct ErrorData {
    pub ident: Ident,
    pub vis: Visibility,
    pub docs: Docs,
    pub is_enum: bool,
    pub backends: Vec<BackendKind>,
    pub backend_options: Vec<BackendOptions>,
    pub span: Span,
}

impl ErrorData {
    pub fn options(&self, kind: BackendKind) -> &[BackendOption] {
        self.backend_options
            .iter()
            .find(|options| options.kind == kind)
            .map(|options| options.entries.as_slice())
            .unwrap_or_default()
    }
}
