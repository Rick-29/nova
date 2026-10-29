use syn::{Generics, Ident, Visibility};

use super::{BindingConfig, Docs, MethodData, MethodKind};

/// Everything known about one indicator in a single macro invocation.
///
/// The derive fills only the struct-level fields. Each `#[bindings]` impl
/// expansion re-reads the struct declaration (handed over by the generated
/// carrier macro) and fills the methods found in its own block.
#[derive(Clone)]
pub struct IndicatorData {
    pub rust_name: Ident,
    /// Foreign-language class name.
    pub public_name: String,
    pub vis: Visibility,
    pub generics: Generics,
    pub config: BindingConfig,
    pub docs: Docs,
    pub constructor: Option<MethodData>,
    pub update: Option<MethodData>,
    pub reset: Option<MethodData>,
    pub ready: Option<MethodData>,
    pub custom: Vec<MethodData>,
}

impl IndicatorData {
    /// Every method recorded in this invocation.
    pub fn methods(&self) -> impl Iterator<Item = &MethodData> {
        self.constructor
            .iter()
            .chain(self.update.iter())
            .chain(self.reset.iter())
            .chain(self.ready.iter())
            .chain(self.custom.iter())
    }

    pub fn slot(&mut self, kind: MethodKind) -> Option<&mut Option<MethodData>> {
        match kind {
            MethodKind::Constructor => Some(&mut self.constructor),
            MethodKind::Update => Some(&mut self.update),
            MethodKind::Reset => Some(&mut self.reset),
            MethodKind::Ready => Some(&mut self.ready),
            MethodKind::Custom => None,
        }
    }
}
