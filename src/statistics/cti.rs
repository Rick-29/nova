use nova_ta_core::{Indicator, Num, Window, math_support};

use crate::{error::{NovaError, NovaResult}, helpers::{sum, w_sum}};

math_support!(math, NovaError::number, T);

/// Correlation Trend Indicator (CTI).
///
/// Measures the Pearson correlation between prices and a linearly increasing
/// time index over a rolling window.
///
/// CTI indicates how closely price follows a straight-line trend:
/// - Values near `1` indicate a strong positive linear trend.
/// - Values near `-1` indicate a strong negative linear trend.
/// - Values near `0` indicate little or no linear correlation.
///
/// CTI measures linear association, not the magnitude of price movement.
/// It is therefore unitless and can be compared across price series with
/// different scales.
///
/// # Calculation
///
/// For `n` observations, let `x` represent prices and `y = 1, 2, ..., n`
/// represent their chronological positions.
///
/// ```text
/// CTI = (n * sum(x * y) - sum(x) * sum(y))
///       / sqrt(
///           (n * sum(x^2) - sum(x)^2)
///           * (n * sum(y^2) - sum(y)^2)
///       )
/// ```
///
/// # Warmup
///
/// The calculation uses the observations currently available in the window.
/// The first update returns zero because correlation cannot be calculated
/// from a single observation. The indicator becomes ready when its configured
/// window is full.
///
/// # Degenerate input
///
/// If the price values have zero variance, correlation is undefined.
/// This implementation returns zero by convention in that case.
pub struct Cti<T: Num> {
    x: Window<T>,
    y: Window<T>,
}

impl<T: Num> Cti<T> {
    /// Creates a CTI indicator with the specified lookback period.
    ///
    /// # Arguments
    ///
    /// - `period`: Number of observations used to calculate the correlation.
    ///   Must be at least two.
    ///
    /// # Errors
    ///
    /// Returns `NovaError::InvalidPeriod` when `period < 2`.
    pub fn new(period: usize) -> NovaResult<Self> {
        if period < 2 {
            return Err(NovaError::InvalidPeriod)
        }
        let mut y = Window::new(period);
        (1..(period +1)).into_iter().for_each(|e| {
            y.push(T::from_usize(e));
        });
        let x = Window::new(period);
        Ok(Self {
            x, 
            y
        })
    }
}

impl<T: Num> Indicator for Cti<T> {
    type Input = T;
    type Output = T;
    type Error = NovaError;

    /// Adds a price observation and returns the CTI value.
    ///
    /// During warmup, CTI is calculated from the available observations.
    /// The first update returns zero because correlation requires at least
    /// two observations.
    ///
    /// Returns zero if the price variance is zero, since correlation is
    /// undefined for a constant price series.
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        self.x.push(input);
        let n = self.x.len();
        if n == 1 {
            return Ok(T::zero())
        }

        // For y = 1, 2, ..., n:
        // n * sum(y^2) - sum(y)^2 = n^2 * (n^2 - 1) / 12.
        let y_det = math!(((n * n * (n * n -1)) / 12) as usize);
        let y_sum = math!((( n * ( n + 1 ) ) / 2) as usize);

        let x_sum = sum(&self.x)?;
        let xy_sum = math!(n as usize * w_sum(&self.x, &self.y)?);
        let x_sum_y_sum = math!(x_sum * y_sum);
        let mut x_sq_sum = T::zero();
        for i in self.x.iter() {
            x_sq_sum = math!(x_sq_sum + (*i) * (*i));
        }
        let x_det = math!(n as usize * x_sq_sum - x_sum * x_sum);

        // A constant series has zero variance, so its correlation is undefined.
        // Return zero by convention rather than producing NaN or infinity.
        if x_det <= T::zero() || y_det <= T::zero() {
            return Ok(T::zero());
        }

        Ok(math!((xy_sum - x_sum_y_sum) / sqrt(x_det * y_det)))
    }

    /// Clears the stored price history.
    ///
    /// The fixed time-index window is retained, allowing the indicator to
    /// be reused without reconstructing it.
    fn reset(&mut self) {
        self.x.clear();
    }

    /// Returns `true` when the price window contains `period` observations.
    fn ready(&self) -> bool {
        self.x.is_full()
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use nova_ta_core::Indicator;

    fn cti(period: usize) -> Cti<f64> {
        Cti::new(period).expect("valid CTI period")
    }

    fn approx_eq(actual: f64, expected: f64) {
        let tolerance = 1e-4;

        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    fn run(values: &[f64]) -> f64 {
        let mut indicator = cti(values.len());
        let mut result = 0.0;

        for &value in values {
            result = indicator.update(value).unwrap();
        }

        result
    }

    #[test]
    fn perfectly_increasing_prices_return_one() {
        approx_eq(run(&[1.0, 2.0, 3.0, 4.0, 5.0]), 1.0);
    }

    #[test]
    fn perfectly_decreasing_prices_return_minus_one() {
        approx_eq(run(&[5.0, 4.0, 3.0, 2.0, 1.0]), -1.0);
    }

    #[test]
    fn constant_prices_return_zero() {
        approx_eq(run(&[7.0, 7.0, 7.0, 7.0, 7.0]), 0.0);
    }

    #[test]
    fn mixed_prices_return_expected_correlation() {
        approx_eq(run(&[1.0, 3.0, 2.0, 5.0, 4.0]), 0.8);
    }

    #[test]
    fn alternating_prices_have_zero_correlation() {
        approx_eq(run(&[2.0, 1.0, 2.0, 1.0, 2.0]), 0.0);
    }

    #[test]
    fn final_price_jump_matches_expected_correlation() {
        approx_eq(run(&[1.0, 2.0, 3.0, 4.0, 10.0]), 0.89442719);
    }

    #[test]
    fn first_update_returns_zero() {
        let mut indicator = cti(5);

        approx_eq(indicator.update(42.0).unwrap(), 0.0);
        assert!(!indicator.ready());
    }

    #[test]
    fn ready_becomes_true_when_window_is_full() {
        let mut indicator = cti(3);

        assert!(!indicator.ready());

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();
        assert!(!indicator.ready());

        indicator.update(3.0).unwrap();
        assert!(indicator.ready());
    }

    #[test]
    fn rolling_window_uses_latest_values() {
        let mut indicator = cti(5);

        for value in [10.0, 11.0, 12.0, 13.0, 14.0] {
            indicator.update(value).unwrap();
        }

        approx_eq(indicator.update(15.0).unwrap(), 1.0);
    }

    #[test]
    fn reset_clears_price_history() {
        let mut indicator = cti(3);

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();
        indicator.update(3.0).unwrap();

        assert!(indicator.ready());

        indicator.reset();

        assert!(!indicator.ready());
        approx_eq(indicator.update(100.0).unwrap(), 0.0);
    }

    #[test]
    fn rejects_period_less_than_two() {
        assert!(Cti::<f64>::new(0).is_err());
        assert!(Cti::<f64>::new(1).is_err());
    }

    #[test]
    fn handles_period_two() {
        approx_eq(run(&[1.0, 2.0]), 1.0);
        approx_eq(run(&[2.0, 1.0]), -1.0);
    }
}
