use syn::{Ident, Path, PathSegment};
use zyn::format_ident;

use crate::ir::{BackendKind, MethodKind};

/// Hidden `macro_rules!` that hands the struct declaration to impl expansions.
pub fn carrier_ident(indicator: &Ident) -> Ident {
    format_ident!("__nova_bindings_{}", indicator)
}

/// Hidden trait proving that `backend` has a binding for `kind`.
pub fn marker_ident(indicator: &Ident, kind: MethodKind, backend: BackendKind) -> Ident {
    let method = match kind {
        MethodKind::Constructor => "New",
        MethodKind::Update => "Update",
        MethodKind::Reset => "Reset",
        MethodKind::Ready => "Ready",
        MethodKind::Custom => unreachable!("custom methods have no completeness marker"),
    };
    format_ident!(
        "__NovaBindings{}{}{}",
        indicator,
        method,
        zyn::case::to_pascal(backend.name())
    )
}

/// `prefix::ident`, where `prefix` is every segment of `target` but the last.
/// Lets impl blocks in another module (`target = super::Cti`) reach the
/// generated items next to the struct.
pub fn sibling(target: &Path, ident: Ident) -> Path {
    let mut path = target.clone();
    path.segments.pop();
    path.segments.push(PathSegment::from(ident));
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn names() {
        let name: Ident = parse_quote!(Cti);
        assert_eq!(carrier_ident(&name), "__nova_bindings_Cti");
        assert_eq!(
            marker_ident(&name, MethodKind::Update, BackendKind::UniFfi),
            "__NovaBindingsCtiUpdateUniffi"
        );
        let path = sibling(&parse_quote!(super::stats::Cti), parse_quote!(PyCti));
        assert_eq!(
            zyn::ToTokens::to_token_stream(&path).to_string(),
            "super :: stats :: PyCti"
        );
    }
}
