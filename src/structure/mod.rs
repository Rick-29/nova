//! Structure: indicators that describe *how* price is moving rather than
//! where it is or how far it moves.
//!
//! An indicator belongs here if it characterises the shape of the market over
//! a lookback window: trending vs. choppy, directional efficiency, swing
//! points, ranges, support/resistance. The output is usually a unitless score
//! or a set of levels, not a smoothed copy of price.
//!
//! Examples: KER (efficiency ratio).
//!
//! Not here: smoothed price lines (see `filters`), pure distribution measures
//! like variance or z-score (see `statistics`), or move-size measures like ATR
//! (see `volatility`).

pub mod ker;
