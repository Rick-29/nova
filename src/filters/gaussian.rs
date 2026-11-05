use nova_bindings_macros::{Bindings, bindings};
use nova_ta_core::{Indicator, Real, Window, real_support};

use crate::{
    error::{NovaError, NovaResult},
    helpers::w_sum,
};

real_support!(real, NovaError::number, R);

/// Finite-window Gaussian smoothing filter.
///
/// Smooths a numeric time series by weighting recent observations according
/// to a Gaussian kernel. The weights are symmetric around the center of the
/// configured window and normalized to sum to one.
///
/// # Parameters
///
/// - `period`: Number of observations in the kernel. Must be a positive, greater than 2,
///   odd integer so the kernel has a central observation.
/// - `sigma`: Positive, finite Gaussian standard deviation controlling the
///   spread of the weights. A smaller value concentrates weight near the
///   center; a larger value distributes weight more broadly.
///
/// # Calculation
///
/// For a period `N`, let `r = (N - 1) / 2`. For each integer index
/// `i` from `-r` to `r`, the raw weight is:
///
/// ```text
/// w[i] = exp(-i^2 / (2 * sigma^2))
/// ```
///
/// The weights are normalized by dividing each raw weight by their sum.
///
/// The output is the weighted sum of the available input observations.
/// During warmup, the weights used are renormalized to sum to one so constant
/// inputs remain constant.
///
/// # Output
///
/// Returns a smoothed value in the same units as the input. The output can
/// therefore be plotted directly over a price series.
///
/// # Readiness
///
/// `ready()` returns `true` once `period` observations have been received.
///
/// # Limitations
///
/// This is a finite-window filter. It uses only the most recent `period`
/// observations and does not use future data.
#[derive(Bindings)]
#[bindings(error = NovaError)]
pub struct Gaussian<R: Real> {
    w: Window<R>,
    x: Window<R>,
}

#[bindings]
impl<R: Real> Gaussian<R> {
    /// Creates a Gaussian filter with the specified period and sigma.
    ///
    /// Returns an error if the period is zero or even, or if sigma is
    /// non-finite or not strictly positive.
    pub fn new(period: usize, sigma: f64) -> NovaResult<Self> {
        if period.is_multiple_of(2) || period < 3 {
            return Err(NovaError::invalid(period, "Period must be an odd number"));
        }
        if sigma <= 0.0 || sigma.is_infinite() || sigma.is_nan() {
            return Err(NovaError::invalid(
                sigma,
                "Sigma should be float greater than 0.0",
            ));
        }
        let s_sq = sigma * sigma;
        let r = ((period - 1) / 2) as isize;
        let raw = (-r..=r)
            .into_iter()
            .map(|e| (-(e * e) as f64 / (2.0 * s_sq)).exp());
        let mut w = Window::new(period);
        let sum: f64 = raw.clone().sum();
        for i in raw {
            w.push(real!((i / sum) as f64));
        }
        let x = Window::new(period);
        Ok(Self { w, x })
    }
}

#[bindings]
impl<R: Real> Indicator for Gaussian<R> {
    type Input = R;
    type Output = R;
    type Error = NovaError;

    /// Adds an input and returns its Gaussian-smoothed value.
    ///
    /// During warmup, the available weights are renormalized so their sum
    /// is one. Once the input window is full, the complete Gaussian kernel
    /// is applied.
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        self.x.push(input);

        // Once the window is full, all normalized Gaussian weights are used.
        // No additional normalization is necessary during normal runtime.
        let weighted_sum = w_sum(&self.x, &self.w)?;

        if self.ready() {
            return Ok(weighted_sum);
        }

        // During warmup, only a subset of the Gaussian weights is used.
        // Renormalize that subset so its weights sum to 1.0 and constant
        // inputs remain unchanged.
        let mut weight_sum = R::zero();
        for (_, weight) in self.x.rev_iter().zip(self.w.rev_iter()) {
            weight_sum = real!(weight_sum + (*weight));
        }

        Ok(real!(weighted_sum / weight_sum))
    }

    fn reset(&mut self) {
        self.x.clear();
    }

    fn ready(&self) -> bool {
        self.x.is_full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_ta_core::Indicator;

    fn gaussian(period: usize, sigma: f64) -> Gaussian<f64> {
        Gaussian::new(period, sigma).expect("valid Gaussian configuration")
    }

    fn approx_eq(actual: f64, expected: f64) {
        let tolerance = 1e-9;
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn constant_input_remains_constant_during_warmup() {
        let mut indicator = gaussian(5, 1.0);

        for _ in 0..10 {
            approx_eq(indicator.update(7.5).unwrap(), 7.5);
        }
    }

    #[test]
    fn symmetric_kernel_preserves_linear_center() {
        let mut indicator = gaussian(3, 1.0);

        // With a symmetric kernel and a full window, a linear series
        // evaluates to the center value.
        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();

        let result = indicator.update(3.0).unwrap();
        approx_eq(result, 2.0);
    }

    #[test]
    fn output_is_between_minimum_and_maximum_for_positive_weights() {
        let mut indicator = gaussian(5, 1.0);
        let values = [10.0, 12.0, 9.0, 15.0, 11.0];

        for value in values {
            let result = indicator.update(value).unwrap();

            // Positive normalized weights form a convex combination.
            assert!(
                (9.0 - 1e-9..=15.0 + 1e-9).contains(&result),
                "output {result} is outside the input range"
            );
        }
    }

    #[test]
    fn responds_to_new_inputs() {
        let mut indicator = gaussian(3, 1.0);

        indicator.update(1.0).unwrap();
        indicator.update(1.0).unwrap();

        let result = indicator.update(10.0).unwrap();

        assert!(result > 1.0);
        assert!(result < 10.0);
    }

    #[test]
    fn ready_becomes_true_when_window_is_full() {
        let mut indicator = gaussian(3, 1.0);

        assert!(!indicator.ready());

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();
        assert!(!indicator.ready());

        indicator.update(3.0).unwrap();
        assert!(indicator.ready());
    }

    #[test]
    fn rolling_window_uses_latest_values() {
        let mut indicator = gaussian(3, 1.0);

        for value in [1.0, 2.0, 3.0, 4.0] {
            indicator.update(value).unwrap();
        }

        // The output should now reflect the window [2, 3, 4].
        // A symmetric Gaussian kernel returns the center of a linear window.
        let result = indicator.update(5.0).unwrap();

        // The window is [3, 4, 5].
        approx_eq(result, 4.0);
    }

    #[test]
    fn reset_clears_history() {
        let mut indicator = gaussian(3, 1.0);

        for value in [1.0, 2.0, 3.0] {
            indicator.update(value).unwrap();
        }

        assert!(indicator.ready());

        indicator.reset();

        assert!(!indicator.ready());
        approx_eq(indicator.update(42.0).unwrap(), 42.0);
        assert!(!indicator.ready());
    }

    #[test]
    fn reset_matches_fresh_instance() {
        let mut reused = gaussian(5, 1.2);

        for value in [10.0, 20.0, 30.0] {
            reused.update(value).unwrap();
        }

        reused.reset();

        let mut fresh = gaussian(5, 1.2);

        for value in [3.0, 1.0, 4.0, 1.0, 5.0] {
            approx_eq(reused.update(value).unwrap(), fresh.update(value).unwrap());
        }
    }

    #[test]
    fn rejects_zero_period() {
        assert!(Gaussian::<f64>::new(0, 1.0).is_err());
    }

    #[test]
    fn rejects_even_period() {
        assert!(Gaussian::<f64>::new(2, 1.0).is_err());
        assert!(Gaussian::<f64>::new(4, 1.0).is_err());
    }

    #[test]
    fn rejects_non_positive_sigma() {
        assert!(Gaussian::<f64>::new(3, 0.0).is_err());
        assert!(Gaussian::<f64>::new(3, -1.0).is_err());
    }

    #[test]
    fn rejects_non_finite_sigma() {
        assert!(Gaussian::<f64>::new(3, f64::NAN).is_err());
        assert!(Gaussian::<f64>::new(3, f64::INFINITY).is_err());
    }
}
