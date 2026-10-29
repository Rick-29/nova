//! Fixture indicators exercising the parts of the binding contract that the
//! Nova indicators do not: overrides, the Python callable policy, exported
//! extra methods, generic constructor arguments, an explicit constructor and
//! impl blocks in another module.

use std::convert::Infallible;

use nova_bindings_macros::{Bindings, bindings, bindings_error};
use nova_ta_core::Indicator;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

/// Error raised by the fixture indicators.
#[bindings_error(python(base = RuntimeError))]
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum FixtureError {
    #[error("period must be at least 1")]
    InvalidPeriod,
    #[error("input must be finite, got {0}")]
    NonFinite(f64),
}

impl From<Infallible> for FixtureError {
    fn from(never: Infallible) -> Self {
        match never {}
    }
}

/// Exponential moving average.
///
/// Seeded with the first input; ready after `period` inputs.
#[derive(Bindings)]
#[bindings(error = FixtureError, name = "Ema", overrides(update(python)), python(update = both))]
pub struct ExpMovingAverage<T> {
    period: usize,
    smoothing: T,
    value: Option<T>,
    seen: usize,
}

#[bindings]
impl<T> ExpMovingAverage<T> {
    /// Creates an EMA with `alpha = smoothing / (period + 1)`.
    pub fn new(period: usize, smoothing: T) -> Result<Self, FixtureError> {
        if period == 0 {
            return Err(FixtureError::InvalidPeriod);
        }
        Ok(Self {
            period,
            smoothing,
            value: None,
            seen: 0,
        })
    }

    /// Number of inputs since construction or the last reset.
    #[bindings(export, name = "samples")]
    pub fn seen(&self) -> usize {
        self.seen
    }

    /// The latest output, if any.
    #[bindings(export)]
    pub fn last(&self) -> Option<T>
    where
        T: Copy,
    {
        self.value
    }
}

#[bindings]
impl<T> Indicator for ExpMovingAverage<T>
where
    T: Copy + Into<f64> + From<f64>,
{
    type Input = T;
    type Output = T;
    type Error = FixtureError;

    /// Feeds one value and returns the average.
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let x: f64 = input.into();
        if !x.is_finite() {
            return Err(FixtureError::NonFinite(x));
        }
        let alpha = self.smoothing.into() / (self.period as f64 + 1.0);
        let next = match self.value {
            None => x,
            Some(previous) => previous.into() + alpha * (x - previous.into()),
        };
        self.value = Some(T::from(next));
        self.seen += 1;
        Ok(T::from(next))
    }

    fn reset(&mut self) {
        self.value = None;
        self.seen = 0;
    }

    fn ready(&self) -> bool {
        self.seen >= self.period
    }
}

/// Python adapter: `update` takes a list and returns one output per value.
/// The numerical work is still the indicator's own `update`.
#[bindings(method = update)]
impl ExpMovingAverage<f64> {
    /// Feeds a batch of values and returns every output.
    pub fn update_batch(&mut self, values: Vec<f64>) -> Result<Vec<f64>, FixtureError> {
        values.into_iter().map(|value| self.update(value)).collect()
    }
}

pub mod counter {
    /// Counts inputs above a threshold. Infallible and not generic.
    #[derive(nova_bindings_macros::Bindings)]
    #[bindings(error = super::FixtureError, backends(python, uniffi, wasm))]
    pub struct Counter {
        pub(crate) threshold: f64,
        pub(crate) count: u32,
    }
}

/// Impl blocks for `counter::Counter`, deliberately in another module.
pub mod counter_impls {
    use nova_bindings_macros::bindings;
    use nova_ta_core::Indicator;

    use super::counter::Counter;

    #[bindings(target = super::counter::Counter)]
    impl Counter {
        /// Creates a counter.
        #[bindings(constructor)]
        pub fn above(threshold: f64) -> Self {
            Self {
                threshold,
                count: 0,
            }
        }

        /// Threshold given at construction.
        #[bindings(export)]
        pub fn threshold(&self) -> f64 {
            self.threshold
        }
    }

    #[bindings(target = super::counter::Counter)]
    impl Indicator for Counter {
        type Input = f64;
        type Output = f64;
        type Error = std::convert::Infallible;

        /// Returns how many inputs so far exceeded the threshold.
        fn update(&mut self, input: f64) -> Result<f64, Self::Error> {
            if input > self.threshold {
                self.count += 1;
            }
            Ok(f64::from(self.count))
        }

        fn reset(&mut self) {
            self.count = 0;
        }

        fn ready(&self) -> bool {
            true
        }
    }
}

#[cfg(feature = "python")]
pub mod python {
    use pyo3::prelude::*;

    pub use crate::PyExpMovingAverage;
    pub use crate::PyFixtureError;
    pub use crate::counter::PyCounter;

    #[pymodule]
    pub fn fixtures(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_class::<PyExpMovingAverage>()?;
        m.add_class::<PyCounter>()?;
        m.add("FixtureError", m.py().get_type::<PyFixtureError>())?;
        Ok(())
    }

    #[cfg(feature = "python-stubs")]
    pyo3_stub_gen::define_stub_info_gatherer!(stub_info);
}
