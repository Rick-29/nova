<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/nova-dark-text.jpg">
    <img alt="Nova: stateful indicators for real-time data" src="assets/nova-light-text.jpg" width="360">
  </picture>
</p>

[![Discord](https://img.shields.io/badge/Discord-Join%20the%20community-5865F2?logo=discord&logoColor=white)](https://discord.gg/QwKmaaeW56)

# Nova

Nova is a stateful technical-analysis library for quantitative finance and real-time market data. Each indicator consumes one sample or candle at a time and keeps its own computation state, so the same code serves candle-by-candle execution, backtesting and live processing. Its focus is on unconventional or under-used indicators (adaptive filters, volatility-normalized measures, market-structure metrics) rather than a catalogue of standard oscillators.

> **Status: early development.** Eight indicators are implemented, built on the shared abstractions in [`nova-ta-core`](https://crates.io/crates/nova-ta-core), and seven of them are also available from Python, Kotlin, Swift, Ruby, JavaScript and Node.js. Planned indicators are tracked in [`TODO.md`](TODO.md). APIs may change.

## Design goals

- **Incremental processing.** Feed one sample at a time instead of re-processing full batches.
- **Stateful indicators.** An indicator owns its state; callers pass no history.
- **Composable.** An indicator built from other indicators holds them as fields and drives them from its own `update` (KAMA drives a KER).
- **Consistent behavior across languages.** Each indicator is implemented once in Rust; every binding calls that same code, so the maths and warm-up semantics are identical in every language.
- **Shared foundation** for trading applications, backtesting and real-time processing.

## Quick start

```rust
use nova_ta::{Alma, error::NovaResult};
use nova_ta_core::Indicator;

fn main() -> NovaResult<()> {
    // period 5, offset 0.85, sigma 6
    let mut alma = Alma::<f64>::new(5, 0.85, 6)?;

    for close in [10.0, 11.0, 12.0, 11.5, 12.5, 13.0] {
        let value = alma.update(close)?;
        if alma.ready() {
            println!("alma = {value:.4}");
        } else {
            println!("warming up (partial = {value:.4})");
        }
    }

    alma.reset();
    assert!(!alma.ready());
    Ok(())
}
```

The `Indicator` trait comes from `nova-ta-core`, which `nova-ta` does not re-export, so add both crates as dependencies.

Key points of the contract:

- `update` always returns a value. Until `ready()` is `true` it is a *partial* result whose seeding each indicator documents; the caller decides whether to use it.
- `update` is fallible: `Result<Output, Error>`. Nova indicators return `NovaError`, where a failing numeric backend surfaces as `NovaError::Number`; with `f64` this never happens. After an error the state is unspecified, so call `reset()` before reuse.
- `reset()` restores the just-constructed state and restarts warm-up.

The same indicator from Python:

```python
from nova_ta import Alma

alma = Alma(5, 0.85, 6)
values = [alma.update(close) for close in [10.0, 11.0, 12.0, 11.5, 12.5, 13.0]]
assert alma.ready()
```

## Available indicators

Every indicator consumes one value per `update` (for example a close) and returns one value of the same type.

| Indicator | Type | Constructor | Description |
| --- | --- | --- | --- |
| ALMA | `Alma<T: Real>` | `new(period, offset, sigma)` | Arnaud Legoux Moving Average: a Gaussian-weighted moving average. |
| KAMA | `Kama<T: Num>` | `new(fast, slow, ker)` | Kaufman Adaptive Moving Average: smoothing speed driven by the Efficiency Ratio. |
| LSMA | `Lsma<T: Num>` | `new(period)` | Least Squares Moving Average: endpoint of a rolling linear regression. |
| WMA | `Wma<T: Num>` | `new(weights)` | Weighted Moving Average with user-supplied weights (Rust only). |
| Linear WMA | `LinearWma<T: Num>` | `new(period)` | WMA with linear weights `1..=period`, newest heaviest. |
| Ultimate Smoother | `UltimateSmoother<R: Real>` | `new(period)` | Ehlers' low-lag recursive low-pass filter. |
| KER | `Ker<T: Num>` | `new(period)` | Kaufman Efficiency Ratio: net movement over total movement, in `[0, 1]`. |
| CTI | `Cti<T: Num>` | `new(period)` | Correlation Trend Indicator: Pearson correlation of price with time, in `[-1, 1]`. |

Constructor rules and warm-up, as implemented (invalid arguments return `NovaError`):

- **ALMA:** `period >= 1`, `offset` in `[0, 1]`, `sigma >= 1`. The weight for position `i` (0 = oldest) is `exp(-(i - m)^2 / (2 s^2))`, with `m = floor(offset * (period - 1))` and `s = period / sigma`. Before `period` values only the newest weights are used, renormalised, so a single value returns itself (up to rounding). `reset()` keeps the weights.
- **KAMA:** `1 <= fast < slow`; `ker` is the Efficiency Ratio period. The first value seeds KAMA and is returned unchanged. `ready()` is `true` once the inner KER window is full.
- **LSMA:** `period >= 1`. Regresses over the values seen so far, so a single value returns itself. Ready after `period` values.
- **WMA:** the number of weights sets the window; weights run oldest to newest and must have a nonzero sum. It takes a `Window<T>`, so it has no foreign bindings.
- **Linear WMA:** `period >= 1`. Same partial-window behavior as WMA. Ready after `period` values.
- **Ultimate Smoother:** `period >= 1`. The first two values are returned unchanged. Ready after three values (the recurrence's history), not after `period` values.
- **KER:** `period >= 1`. The first update returns `0`, and so does a flat window. Ready after `period` values.
- **CTI:** `period >= 2`. The first update returns `0`, and so do constant prices. Ready after `period` values.

Planned work, in priority order: a Gaussian filter, structure measures (Vertical Horizontal Filter, Donchian breakout), robust statistics (rolling regression slope and R², robust Z-score) and volatility-normalized indicators. See [`TODO.md`](TODO.md).

## Architecture

- **`nova-ta-core`** defines the abstractions: the `Indicator` trait (`update`, `ready`, `reset`), the `Num` and `Real` numeric traits with fallible arithmetic (implemented for `f64`), the `Ohlc<N>` candle type and the fixed-capacity rolling `Window<T>`. It contains no indicators. See its [README](https://crates.io/crates/nova-ta-core) for details.
- **`nova-ta`** (this crate) holds the concrete indicators, written once against those traits, plus `NovaError` / `NovaResult`. Source layout: `src/filters` (ALMA, KAMA, LSMA, WMA, Linear WMA, Ultimate Smoother), `src/structure` (KER), `src/statistics` (CTI), `src/volatility`.
- **[`nova-bindings-macros`](nova-bindings-macros/README.md)** generates the foreign-language wrappers from `#[derive(Bindings)]` and `#[bindings]` annotations on each indicator. The wrappers only delegate to the Rust code.

## Language interoperability

Every indicator except WMA is exported to each binding under its Rust name (`Alma`, `Cti`, ...), with `f64` as the numeric type. Each binding is a Cargo feature of `nova-ta`:

| Feature | Languages | Built with |
| --- | --- | --- |
| `python` (+ `python-stubs` for `.pyi` stubs) | Python 3.9+ | [maturin](https://www.maturin.rs), [pyo3-stub-gen](https://crates.io/crates/pyo3-stub-gen) |
| `uniffi` | Kotlin / Android, Swift / iOS / macOS, Ruby, Python | [UniFFI](https://github.com/mozilla/uniffi-rs) |
| `wasm` | JavaScript (browser, Node.js, bundlers) | [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) |
| `napi` | Node.js native addon | [napi-rs](https://napi.rs) |

Errors keep the `NovaError` message in every language: Python raises `nova_ta.NovaError` (a `ValueError`), Kotlin throws `NovaException`, Swift and Ruby `NovaError`, and JavaScript an `Error`.

[`COMPILE.md`](COMPILE.md) lists the commands that build each binding into `bindings/<language>/`, for example the Python wheel:

```sh
cargo run --release --bin stub_gen --features python-stubs   # type stubs
maturin build --release --out bindings/python
```

## Chipa integration

Nova is being built to support the [Chipa](https://chipaeditor.com) ecosystem, in particular the new Chipa-Flow language for expressing trading strategies, where indicators are building blocks. This is work in progress: the Chipa-Flow version described on the public website does not use Nova, and Nova is meant to remain useful as a standalone library. A beta is available in [Chipa Studio](https://studio.chipaeditor.com).

## Installation and development

```sh
cargo add nova-ta
```

To work on Nova itself:

```sh
git clone https://github.com/Rick-29/nova.git
cd nova
cargo build
cargo test --workspace
cargo fmt
cargo doc --no-deps
```

The `python` and `python-stubs` features are on by default, so these commands need a Python interpreter; add `--no-default-features` for a pure-Rust build.

To test the bindings:

```sh
maturin develop && python -m unittest discover python/tests   # Python extension
sh scripts/uniffi-python-test.sh                              # UniFFI-generated bindings
cargo test -p nova-bindings-macros                            # macro unit and compile-fail tests
```

## Using Nova?

Nova is free to use in personal, commercial and closed-source projects. If you build something with it, we'd love to hear about it (a project name or link is plenty); there are no fees or obligations.

## Help and community

Questions or feedback? Join the [Nova Discord](https://discord.gg/QwKmaaeW56).

## License

Licensed under the [MIT license](LICENSE).
