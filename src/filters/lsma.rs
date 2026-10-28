use nova_ta_core::{Indicator, Num, Window, math_support, };

use crate::{
    error::{NovaError, NovaResult},
    helpers::add,
};

// math_impl! macro
// Fn(Erreur) -> CustomError, T
math_support!(math, NovaError::number, T);
// math!(expr) -> math_impl(NovaErro::number, T, expr)
/// Least Squares Moving Average (LSMA).
///
/// Fits a least-squares linear regression to the values in the rolling window
/// and returns the fitted line's value at the newest position.
///
/// The regression uses integer-spaced x-coordinates starting at 1 for the
/// oldest observation and ending at the current window length for the newest.
///
/// # Calculation
///
/// For `n` observations, the fitted line is:
///
/// ```text
/// y = m * x + b
///
/// m = (n * sum(x * y) - sum(x) * sum(y))
///     / (n * sum(x * x) - sum(x)^2)
///
/// b = (sum(y) - m * sum(x)) / n
///
/// LSMA = m * n + b
/// ```
///
/// # Warmup
///
/// The indicator calculates using the values currently in the window, even
/// before it is full. For a single observation, it returns that observation.
///
/// Use [`Indicator::ready`] to determine when the configured period has been
/// reached.
pub struct Lsma<T: Num> {
    candles: Window<T>,
}

impl<T: Num> Lsma<T> {
    /// Creates an LSMA indicator with the specified lookback period.
    ///
    /// # Arguments
    ///
    /// * `period` - Maximum number of observations used for regression.
    ///
    /// # Errors
    ///
    /// Returns an error if `period` is invalid.
    pub fn new(period: usize) -> NovaResult<Self> {
        NovaError::period(period)?;
        Ok(Self {
            candles: Window::new(period),
        })
    }
}

impl<T: Num + std::fmt::Debug> Indicator for Lsma<T> {
    type Input = T;
    type Output = T;
    type Error = NovaError;

    /// Adds a new observation and returns the fitted regression value at the
    /// newest position.
    ///
    /// When the window is not yet full, regression uses all available values.
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        self.candles.push(input);
        if self.candles.len() == 1 {
            return Ok(input);
        }
        let n = self.candles.len();
        let x_usum = n * (n + 1) / 2;
        let x_sum = math!(x_usum as usize);
        let x_sum2 = math!((x_usum * x_usum) as usize);
        let x_sq2 = math!((n * (n + 1) * (2 * n + 1) / 6) as usize);
        let y_sum = self
            .candles
            .iter()
            .try_fold(T::zero(), |acc, e| add(&acc, e))?;
        // sum(x * y)
        let mut xy_sum = T::zero();
        
        for (i, &value) in self.candles.iter().enumerate() {
            let x = math!((i + 1) as usize);
            xy_sum = math!(xy_sum + x * value);
            // xy_sum = add(&xy_sum, &mul(&x, &value)?)?;
        }

        let n_t = math!(n as usize);
        let m = math!(((n_t * xy_sum) - (x_sum * y_sum)) / (n_t * x_sq2 - x_sum2));
        // let m = div(
        //     &sub(&mul(&n_t, &xy_sum)?, &mul(&x_sum, &y_sum)?)?,
        //     &sub(&mul(&n_t, &x_sq2)?, &x_sum2)?,
        // )?;
        let b = math!((y_sum - m * x_sum) / n_t);
        // let b = div(&sub(&y_sum, &mul(&m, &x_sum)?)?, &n_t)?;
        Ok(math!(m * n_t + b))
        // add(&mul(&m, &n_t)?, &b)
    }

    /// Returns `true` when the rolling window contains `period` observations.
    fn ready(&self) -> bool {
        self.candles.is_full()
    }

    /// Clears all observations, allowing the indicator to start fresh.
    fn reset(&mut self) {
        self.candles.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_ta_core::Indicator;

    fn lsma(period: usize) -> Lsma<f64> {
        Lsma::new(period).expect("valid period")
    }

    fn approx_eq(actual: f64, expected: f64) {
        let tolerance = 1e-10;

        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn first_update_returns_input() {
        let mut indicator = lsma(3);

        approx_eq(indicator.update(42.0).unwrap(), 42.0);
    }

    #[test]
    fn linear_increasing_values_return_latest_value() {
        let mut indicator = lsma(3);

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();

        let result = indicator.update(3.0).unwrap();

        // The fitted line is y = x.
        approx_eq(result, 3.0);
    }

    #[test]
    fn linear_decreasing_values_return_latest_value() {
        let mut indicator = lsma(3);

        indicator.update(3.0).unwrap();
        indicator.update(2.0).unwrap();

        let result = indicator.update(1.0).unwrap();

        // The fitted line is y = 4 - x.
        approx_eq(result, 1.0);
    }

    #[test]
    fn calculates_least_squares_for_non_linear_data() {
        let mut indicator = lsma(3);

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();

        let result = indicator.update(4.0).unwrap();

        // Points: (1, 1), (2, 2), (3, 4).
        // Slope = 1.5; intercept = -2/3.
        // Fitted value at x = 3 is 23/6.
        approx_eq(result, 23.0 / 6.0);
    }

    #[test]
    fn constant_values_return_the_constant() {
        let mut indicator = lsma(4);

        indicator.update(5.0).unwrap();
        indicator.update(5.0).unwrap();
        indicator.update(5.0).unwrap();

        approx_eq(indicator.update(5.0).unwrap(), 5.0);
    }

    #[test]
    fn uses_partial_window_during_warmup() {
        let mut indicator = lsma(4);

        assert!(!indicator.ready());

        approx_eq(indicator.update(1.0).unwrap(), 1.0);
        approx_eq(indicator.update(2.0).unwrap(), 2.0);
        approx_eq(indicator.update(3.0).unwrap(), 3.0);

        assert!(!indicator.ready());
    }

    #[test]
    fn ready_becomes_true_when_window_is_full() {
        let mut indicator = lsma(3);

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();

        assert!(!indicator.ready());

        indicator.update(3.0).unwrap();

        assert!(indicator.ready());
    }

    #[test]
    fn rolling_window_discards_oldest_value() {
        let mut indicator = lsma(3);

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();
        indicator.update(3.0).unwrap();

        // The window now contains [2, 3, 4].
        let result = indicator.update(4.0).unwrap();

        approx_eq(result, 4.0);
    }

    #[test]
    fn reset_clears_window() {
        let mut indicator = lsma(3);

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();
        indicator.update(3.0).unwrap();

        assert!(indicator.ready());

        indicator.reset();

        assert!(!indicator.ready());

        approx_eq(indicator.update(100.0).unwrap(), 100.0);
        assert!(!indicator.ready());
    }

    #[test]
    fn rejects_zero_period() {
        assert!(Lsma::<f64>::new(0).is_err());
    }

    #[test]
    fn period_one_always_returns_latest_input() {
        let mut indicator = lsma(1);

        approx_eq(indicator.update(10.0).unwrap(), 10.0);
        approx_eq(indicator.update(20.0).unwrap(), 20.0);
        approx_eq(indicator.update(-3.0).unwrap(), -3.0);

        assert!(indicator.ready());
    }
}
