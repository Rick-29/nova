//! Nova: streaming technical-analysis indicators.
//!
//! Every indicator implements [`nova_ta_core::Indicator`]: feed it one sample
//! at a time with `update`, check `ready` to know when warm-up is complete, and
//! call `reset` to start over. `update` always returns a value, even during
//! warm-up, so callers decide whether to skip the partial outputs.
//!
//! # Indicators
//!
//! - **Filters** (price-scaled smoothing lines): [`Alma`], [`Kama`], [`Lsma`],
//!   [`UltimateSmoother`].
//! - **Structure**: [`Ker`] (Kaufman efficiency ratio).
//!
//! # Example
//!
//! ```
//! use nova_ta::{Lsma};
//! use nova_ta_core::Indicator;
//!
//! let mut lsma = Lsma::<f64>::new(3).unwrap();
//! lsma.update(1.0).unwrap();
//! lsma.update(2.0).unwrap();
//! let value = lsma.update(3.0).unwrap();
//!
//! assert!(lsma.ready());
//! assert!((value - 3.0).abs() < 1e-10);
//! ```

pub(crate) mod filters;
pub(crate) mod statistics;
pub(crate) mod structure;
pub(crate) mod volatility;

/// Error type and result alias shared by all indicators.
pub mod error;
/// Minimal functions that might be useful.
pub mod helpers;

pub use filters::alma::Alma;
pub use filters::kama::Kama;
pub use filters::lsma::Lsma;
pub use filters::ultimate_smoother::UltimateSmoother;

/// Re-exports from `nova_ta_core` used in indicator signatures.
pub use nova_ta_core::{Num, Ohlc, Real};
pub use structure::ker::Ker;
