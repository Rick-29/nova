# Indicator roadmap

Nova does not try to be a catalogue of conventional oscillators. The aim is
unconventional or under-used indicators that may behave better than the usual
suspects, so priority goes to:

1. **Adaptive filters**: smoothing that changes with market conditions.
2. **Volatility-normalized indicators**: outputs comparable across regimes and instruments.
3. **Market-structure indicators**: measures of trend quality, range and breakout rather than raw momentum.

Only items ticked below are implemented. For every indicator, document the exact
seeding and warm-up convention, and test it against a hand-computed series.

Planned layout (`src/`): `filters/`, `volatility/`, `structure/`, `statistics/`.

## Filters (`src/filters/`)

- [x] **ALMA** (Arnaud Legoux Moving Average): `src/filters/alma.rs`, `m = floor(offset * (period - 1))`.
- [x] **KAMA** (Kaufman Adaptive Moving Average): smoothing speed driven by the Efficiency Ratio. Depends on ER.
- [x] **LSMA** (Least Squares Moving Average): endpoint of a rolling linear regression.
- [x] **WMA** (Weighted Moving Average): linear weights over a finite window; baseline for custom weight profiles.
- [ ] **Gaussian filter**: decide between finite-window and recursive form before implementing.

## Structure (`src/structure/`)

- [x] **Efficiency Ratio (ER)**: net directional movement divided by total absolute movement over a window. Useful alone as a trend-quality feature and as a KAMA component.
- [ ] **Vertical Horizontal Filter (VHF)**: range covered over a window relative to the sum of individual movements; separates directional from choppy phases.
- [ ] **Donchian Channels** and prior-window breakout: rolling high/low. Decide and document whether the current candle is included (prior-candle semantics).

## Statistics (`src/statistics/`)

- [ ] **Rolling linear regression slope and R²**: slope gives direction and rate; R² gives fit quality of the linear trend, not predictive power.
- [ ] **Robust Z-score**: rolling median and median absolute deviation (MAD), less sensitive to outliers than mean/standard-deviation normalization. Document the MAD scaling constant if used.

## Volatility (`src/volatility/`)

- [ ] Volatility-normalized indicators (to be defined).

## Open questions

- Re-export `Indicator`, `Ohlc` and `Real` from `nova` so users need only one dependency.
- Extend ALMA with a selectable price source (currently `close` only).
- Add Rustdoc for `Alma`, `NovaError` and `helpers` (currently undocumented).
- Shared helpers for rolling extrema and rolling median on top of `nova_ta_core::Window`.
- Which indicators take `f64` / `Num` prices and which take `Ohlc`.
