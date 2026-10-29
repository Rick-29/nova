//! Statistics: indicators that are direct statistical measures of a price
//! window, independent of any trading interpretation.
//!
//! An indicator belongs here if it is a standard statistical quantity computed
//! over a rolling window: mean, variance, standard deviation, z-score,
//! skewness, kurtosis, correlation, linear-regression slope or R².
//!
//! Not here: statistics used to build a price-scaled smoother (see `filters`),
//! market-shape scores (see `structure`), or measures of price-move magnitude
//! (see `volatility`; standard deviation used as a volatility gauge may live
//! there instead).
//!
//! No indicators yet.
pub mod cti;