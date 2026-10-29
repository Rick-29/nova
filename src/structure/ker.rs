use nova_bindings_macros::{Bindings, bindings};
use nova_ta_core::{Indicator, Num, Window};

use crate::{
    error::{NovaError, NovaResult},
    helpers::{add, sub},
};

/// Kaufman Efficiency Ratio (KER).
///
/// Measures how efficiently price moves in one direction over a lookback
/// period, relative to the total absolute movement during that period.
///
/// The ratio is:
/// ```no_compile
///     direction / volatility
/// ```
/// where:
/// - `direction` is the absolute difference between the newest and oldest values.
/// - `volatility` is the sum of absolute differences between consecutive values.
///
/// The result is generally between 0 and 1 when volatility is nonzero:
/// - Values closer to 1 indicate a more directional movement.
/// - Values closer to 0 indicate more back-and-forth movement.
#[derive(Bindings)]
#[bindings(error = NovaError)]
pub struct Ker<T: Num> {
    items: Window<T>,
}

#[bindings]
impl<T: Num> Ker<T> {
    /// Creates a KER indicator with the specified lookback period.
    ///
    /// Returns an error if the period is invalid.
    pub fn new(period: usize) -> NovaResult<Self> {
        NovaError::period(period)?;
        Ok(Self {
            items: Window::new(period),
        })
    }
}

#[bindings]
impl<T: Num> Indicator for Ker<T> {
    type Input = T;
    type Output = T;
    type Error = NovaError;

    /// Processes a new value and calculates the efficiency ratio.
    ///
    /// On the first call, returns `T::zero()` without storing the input,
    /// following the current implementation's initialization convention.
    ///
    /// On flat window, (volatility = 0) then the indicator returns `T::zero()` instead of an error.
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        if self.items.is_empty() {
            self.items.push(input);
            return Ok(T::zero());
        }
        self.items.push(input);
        let mut prev = self.items.oldest().ok_or(NovaError::never())?;
        let dir = input.sub(*prev).map_err(NovaError::number)?.abs();
        let mut vol = T::zero();

        for curr in self.items.iter().skip(1) {
            vol = add(&vol, &sub(curr, prev)?.abs())?;
            prev = curr;
        }
        if vol == T::zero() {
            return Ok(T::zero());
        }
        dir.div(vol).map_err(NovaError::number)
    }

    fn ready(&self) -> bool {
        self.items.is_full()
    }

    fn reset(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_ta_core::Indicator;

    fn ker(period: usize) -> Ker<f64> {
        Ker::new(period).expect("valid period")
    }

    fn approx_eq(actual: f64, expected: f64) {
        let tolerance = 1e-10;
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn first_update_returns_zero() {
        let mut indicator = ker(3);

        let result = indicator.update(10.0).unwrap();

        assert_eq!(result, 0.0);
    }

    #[test]
    fn steadily_increasing_values_have_ratio_one() {
        let mut indicator = ker(3);

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();
        indicator.update(3.0).unwrap();

        let result = indicator.update(4.0).unwrap();

        // For [2, 3, 4]:
        // direction = |4 - 2| = 2
        // volatility = |3 - 2| + |4 - 3| = 2
        approx_eq(result, 1.0);
    }

    #[test]
    fn steadily_decreasing_values_have_ratio_one() {
        let mut indicator = ker(3);

        indicator.update(4.0).unwrap();
        indicator.update(3.0).unwrap();
        indicator.update(2.0).unwrap();

        let result = indicator.update(1.0).unwrap();

        approx_eq(result, 1.0);
    }

    #[test]
    fn oscillating_values_have_lower_efficiency() {
        let mut indicator = ker(3);

        indicator.update(1.0).unwrap();
        indicator.update(3.0).unwrap();
        indicator.update(1.0).unwrap();

        let result = indicator.update(3.0).unwrap();

        // For [3, 1, 3]:
        // direction = |3 - 3| = 0
        // volatility = |1 - 3| + |3 - 1| = 4
        approx_eq(result, 0.0);
    }

    #[test]
    fn result_is_between_zero_and_one_for_nonflat_data() {
        let mut indicator = ker(4);
        let values = [10.0, 12.0, 11.0, 14.0, 13.0, 16.0];

        for value in values {
            let result = indicator.update(value).unwrap();

            if result.is_finite() {
                assert!((0.0..=1.0).contains(&result), "KER out of range: {result}");
            }
        }
    }

    #[test]
    fn flat_values_return_zero() {
        let mut indicator = ker(3);

        indicator.update(5.0).unwrap();
        indicator.update(5.0).unwrap();
        indicator.update(5.0).unwrap();

        let result = indicator.update(5.0).unwrap();

        approx_eq(result, 0.0);
    }

    #[test]
    fn ready_becomes_true_when_window_is_full() {
        let mut indicator = ker(3);

        assert!(!indicator.ready());

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();
        indicator.update(3.0).unwrap();

        assert!(indicator.ready());
    }

    #[test]
    fn reset_clears_state() {
        let mut indicator = ker(3);

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();
        indicator.update(3.0).unwrap();

        indicator.reset();

        assert!(!indicator.ready());
        assert_eq!(indicator.update(100.0).unwrap(), 0.0);
    }

    #[test]
    fn rejects_zero_period() {
        assert!(Ker::<f64>::new(0).is_err());
    }
}
