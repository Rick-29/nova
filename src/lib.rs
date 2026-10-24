pub(crate) mod filters;
pub(crate) mod statistics;
pub(crate) mod structure;
pub(crate) mod volatility;

pub mod error;
/// Minimal functions the might be useful
pub mod helpers;

pub use filters::alma::Alma;
pub use filters::ker::Ker;