use nova_ta_core::{Indicator, Real, Window, real_support};

use crate::error::{NovaResult, NovaError};

real_support!(real, NovaError::number, R);

/// Ehlers' Ultimate Smoother.
///
/// A recursive low-pass filter designed by John Ehlers to smooth price data
/// while reducing the lag associated with conventional smoothing filters.
///
/// The indicator accepts one price value per update and returns a smoothed
/// value in the same units as the input. It maintains the previous three
/// input values and the previous three output values needed by its recurrence.
///
/// # Calculation
///
/// For a configured period `N`, the coefficients are:
///
/// ```text
/// f  = sqrt(2) * PI / N
/// a1 = exp(-f)
///
/// c2 =  2 * a1 * cos(f)
/// c3 = -a1^2
/// c1 = (1 + c2 - c3) / 4
/// ```
///
/// For the current input `x[t]`, the output is:
///
/// ```text
/// US[t] = (1 - c1) * x[t]
///       + (2 * c1 - c2) * x[t-1]
///       - (c1 + c3) * x[t-2]
///       + c2 * US[t-1]
///       + c3 * US[t-2]
/// ```
///
/// The first two outputs are initialized directly from their corresponding
/// inputs because there is not yet enough input history to apply the full
/// recurrence.
///
/// # Inputs
///
/// - `input: R` — the current price or other numeric series value.
/// - `period: usize` — the smoothing period used to calculate the coefficients.
///
/// # Output
///
/// Returns the current smoothed value as `R`. The output remains in the same
/// units as the input, so the indicator can be plotted directly over price.
///
/// # Warmup and readiness
///
/// The first two updates return their inputs unchanged. The third update can
/// apply the full recurrence.
///
/// [`Indicator::ready`] becomes `true` after three outputs have been stored.
/// This indicates that the internal output history is full; it does not mean
/// that `period` updates have elapsed.
///
/// # Errors
///
/// Construction returns an error if the period is invalid or coefficient
/// calculations fail. Updates can return an error if numeric operations fail.
pub struct UltimateSmoother<R: Real> {
    c1: R,
    c2: R,
    c3: R,
    x: Window<R>,
    us: Window<R>
}


impl<R: Real> UltimateSmoother<R> {
    /// Creates an Ultimate Smoother with the specified period.
    ///
    /// The filter coefficients are calculated once during construction and
    /// reused for every subsequent update.
    ///
    /// # Arguments
    ///
    /// - `period` — smoothing period; must satisfy [`NovaError::period`].
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let smoother = UltimateSmoother::<f64>::new(10)?;
    /// ```
    pub fn new(period: usize) -> NovaResult<Self> {
        NovaError::period(period)?;
        let x = Window::new(3);
        let us = Window::new(3);
        let f = real!(sqrt(2) * PI / period as usize);
        let a1 = real!(exp(-f));
        let c2 = real!(2 * a1 * cos(f));
        let c3 = real!(- a1 ** 2);
        let c1 = real!((1 + c2 - c3) / 4);
        Ok(Self {
          c1,
          c2,
          c3, 
          x,
          us
        })
    }
}

impl<R: Real> Indicator for UltimateSmoother<R> {
    type Input = R;
    type Output = R;
    type Error = NovaError;
    
    /// Processes a new input and returns the smoothed output.
    ///
    /// The first two updates return the input unchanged. Once three inputs
    /// have been received, the recursive filter uses the current input,
    /// two previous inputs, and two previous outputs.
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        self.x.push(input);
        let us3 = match self.x.len() {
            // 0 is unreachable but for considering edge case
            0 | 1 | 2 => input,
            _ => {
                let p0 = self.x.get(0).ok_or(NovaError::never())?;
                let p1 = self.x.get(1).ok_or(NovaError::never())?;
                let us0 = self.us.get_from_newest(1).ok_or(NovaError::never())?;
                let us1 = self.us.get_from_newest(0).ok_or(NovaError::never())?;
                // US = (1 - c1) * x + (2 * c1 - c2) * x[1] - (c1 + c3) * x[2]
                //      + c2 * US[1] + c3 * US[2]
                real!(
                    (1 - self.c1) * input
                        + (2 * self.c1 - self.c2) * (*p1)
                        - (self.c1 + self.c3) * (*p0)
                        + self.c2 * (*us1)
                        + self.c3 * (*us0)
                )
            }
        };
        // Store the result for use in subsequent recursive updates.
        self.us.push(us3);
        
        Ok(us3)
    }

    /// Returns `true` once three output values have been stored.
    ///
    /// This reflects the history required by the implementation, not the
    /// configured smoothing period.
    fn ready(&self) -> bool {
        self.us.is_full()
    }

    /// Clears the input and output history.
    ///
    /// The coefficients remain unchanged, so the indicator can be reused
    /// with the same period without reconstructing it.
    fn reset(&mut self) {
        self.us.clear();
        self.x.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use nova_ta_core::Indicator;

    fn smoother(period: usize) -> UltimateSmoother<f64> {
        UltimateSmoother::new(period).expect("valid period")
    }

    fn approx_eq(actual: f64, expected: f64) {
        let tolerance = 1e-9;

        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    /// Independent f64 implementation of Ehlers' Ultimate Smoother.
    fn reference(period: usize, input: &[f64]) -> Vec<f64> {
        let f = 2f64.sqrt() * std::f64::consts::PI / period as f64;
        let a1 = (-f).exp();
        let c2 = 2.0 * a1 * f.cos();
        let c3 = -a1 * a1;
        let c1 = (1.0 + c2 - c3) / 4.0;

        let mut us: Vec<f64> = Vec::with_capacity(input.len());
        for (i, &x) in input.iter().enumerate() {
            if i < 2 {
                us.push(x);
            } else {
                us.push(
                    (1.0 - c1) * x + (2.0 * c1 - c2) * input[i - 1] - (c1 + c3) * input[i - 2]
                        + c2 * us[i - 1]
                        + c3 * us[i - 2],
                );
            }
        }
        us
    }

    #[test]
    fn warmup_returns_input() {
        let mut indicator = smoother(10);

        approx_eq(indicator.update(7.0).unwrap(), 7.0);
        approx_eq(indicator.update(9.0).unwrap(), 9.0);
    }

    #[test]
    fn ready_after_three_updates() {
        let mut indicator = smoother(10);

        assert!(!indicator.ready());
        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();
        assert!(!indicator.ready());
        indicator.update(3.0).unwrap();
        assert!(indicator.ready());
    }

    #[test]
    fn constant_input_returns_the_constant() {
        let mut indicator = smoother(14);

        for _ in 0..100 {
            approx_eq(indicator.update(5.0).unwrap(), 5.0);
        }
    }

    #[test]
    fn linear_input_has_no_lag() {
        // The Ultimate Smoother's passband has zero lag, so a ramp passes through
        // unchanged once the transient has died out.
        let mut indicator = smoother(10);
        let mut last = 0.0;

        for i in 0..200 {
            last = indicator.update(i as f64).unwrap();
        }

        assert!((last - 199.0).abs() < 1e-6, "got {last}");
    }

    #[test]
    fn matches_reference_implementation() {
        let input: Vec<f64> = (0..60)
            .map(|i| 100.0 + (i as f64 * 0.7).sin() * 5.0 + i as f64 * 0.1)
            .collect();

        for period in [2, 5, 10, 20] {
            let expected = reference(period, &input);
            let mut indicator = smoother(period);

            for (i, &x) in input.iter().enumerate() {
                approx_eq(indicator.update(x).unwrap(), expected[i]);
            }
        }
    }

    #[test]
    fn attenuates_high_frequency_noise() {
        // Alternating input is the highest-frequency signal; the output swing
        // should be far smaller than the input swing.
        let mut indicator = smoother(20);
        let mut outputs = Vec::new();

        for i in 0..200 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            outputs.push(indicator.update(x).unwrap());
        }

        let tail = &outputs[150..];
        let max = tail.iter().cloned().fold(f64::MIN, f64::max);
        let min = tail.iter().cloned().fold(f64::MAX, f64::min);

        assert!(max - min < 0.2, "swing was {}", max - min);
    }

    #[test]
    fn reset_restarts_warmup() {
        let mut indicator = smoother(10);

        for i in 0..10 {
            indicator.update(i as f64).unwrap();
        }
        assert!(indicator.ready());

        indicator.reset();
        assert!(!indicator.ready());

        approx_eq(indicator.update(42.0).unwrap(), 42.0);
        approx_eq(indicator.update(43.0).unwrap(), 43.0);
    }

    #[test]
    fn reset_matches_fresh_instance() {
        let input = [3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0];
        let mut reused = smoother(6);
        for &x in &[10.0, 20.0, 30.0, 40.0] {
            reused.update(x).unwrap();
        }
        reused.reset();

        let mut fresh = smoother(6);
        for &x in &input {
            approx_eq(reused.update(x).unwrap(), fresh.update(x).unwrap());
        }
    }

    #[test]
    fn rejects_zero_period() {
        assert!(UltimateSmoother::<f64>::new(0).is_err());
    }
}
