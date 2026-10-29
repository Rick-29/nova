//! Backend-neutral code generation shared by all backends: naming,
//! generic instantiation, delegation calls, completeness markers and the
//! configuration carrier.

pub mod carrier;
pub mod delegate;
pub mod markers;
pub mod naming;

use zyn::TokenStream;
use zyn::proc_macro2::{Delimiter, Group};

/// `{ tokens }` as a single group.
///
/// Needed because a template brace group holding only an interpolation,
/// `{ {{ x }} }`, is read by `zyn!` as the interpolation itself.
pub fn braced(tokens: TokenStream) -> Group {
    Group::new(Delimiter::Brace, tokens)
}
