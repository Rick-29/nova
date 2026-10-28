//! Filters: indicators that smooth a price series and return a price-scaled
//! line (same units as the input).
//!
//! An indicator belongs here if its output is a *replacement* for the input
//! series, typically a moving average or similar low-pass filter that trades
//! lag for noise reduction. The output can be plotted directly over price.
//!
//! Examples: ALMA, KAMA, LSMA.
//!
//! Not here: indicators whose output is a unitless ratio or score (see
//! `structure` / `statistics`), or that measure the size of price moves (see
//! `volatility`). KAMA uses KER internally, but KER itself is not a filter.

pub mod alma;
pub mod kama;
pub mod lsma;
pub mod wma;
pub mod ultimate_smoother;