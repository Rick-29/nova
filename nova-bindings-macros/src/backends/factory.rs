use crate::backends::BackendBuilder;
use crate::backends::napi::NapiBackend;
use crate::backends::python::PythonBackend;
use crate::backends::uniffi::UniFfiBackend;
use crate::backends::wasm::WasmBackend;
use crate::ir::BackendKind;

/// The single place mapping a backend kind to its builder.
pub fn builder(kind: BackendKind) -> &'static dyn BackendBuilder {
    match kind {
        BackendKind::Python => &PythonBackend,
        BackendKind::UniFfi => &UniFfiBackend,
        BackendKind::Wasm => &WasmBackend,
        BackendKind::Napi => &NapiBackend,
    }
}

/// Builders for `kinds`, in canonical order.
pub fn selected(kinds: &[BackendKind]) -> Vec<&'static dyn BackendBuilder> {
    BackendKind::ALL
        .into_iter()
        .filter(|kind| kinds.contains(kind))
        .map(builder)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_builder() {
        for kind in BackendKind::ALL {
            assert_eq!(builder(kind).kind(), kind);
        }
    }

    #[test]
    fn selection_follows_canonical_order() {
        let kinds: Vec<_> = selected(&[BackendKind::Napi, BackendKind::Python])
            .iter()
            .map(|builder| builder.kind())
            .collect();
        assert_eq!(kinds, [BackendKind::Python, BackendKind::Napi]);
    }
}
