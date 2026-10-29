//! Project-owned intermediate representation.
//!
//! The IR records source-derived facts and binding intent. It holds no
//! backend-specific wrapper types; backends consume it to generate code.

mod config;
mod docs;
mod error;
mod indicator;
mod method;
mod types;

pub use config::{
    BackendKind, BackendOption, BackendOptions, BindingConfig, OptionValue, OverrideDecl,
};
pub use docs::Docs;
pub use error::ErrorData;
pub use indicator::IndicatorData;
pub use method::{ArgData, CallTarget, MethodData, MethodKind, Receiver, ReturnData};
pub use types::{BindingType, ScalarType, display_type};
