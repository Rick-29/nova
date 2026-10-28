use nova_ta_core::{Indicator, Num, Window, math_support};

use crate::error::{NovaError, NovaResult};

math_support!(math, NovaError::number, T);

/// Weighted Moving Average (WMA).
///
/// Calculates a weighted average of recent input values using a fixed window
/// of user-provided weights. The newest input is paired with the newest
/// weight, the preceding input with the preceding weight, and so on.
///
/// # Calculation
///
/// For the available inputs and corresponding weights:
///
/// ```text
/// WMA = sum(input[i] * weight[i]) / sum(weight[i])
/// ```
///
/// # Warmup
///
/// Before the input window is full, the calculation uses only the available
/// inputs and their corresponding newest weights. The result is normalized
/// by the sum of the weights actually used.
///
/// # Notes
///
/// The weights must be non-empty and their sum must be nonzero for a valid
/// result. This constructor does not validate those conditions.
pub struct Wma<T: Num> {
    x: Window<T>,
    weights: Window<T>
}

impl<T: Num> Wma<T> {
    /// Creates a WMA using the supplied weights.
    ///
    /// The number of weights determines the input window capacity.
    ///
    /// Weights are interpreted in oldest-to-newest order. The newest input
    /// is multiplied by the last weight in the supplied window.
    ///
    /// The caller must provide at least one weight and ensure the weights
    /// have a nonzero sum.
    pub fn new(weights: Window<T>) -> NovaResult<Self> {
        NovaError::period(weights.len())?;
        let x = Window::new(weights.len());
        Ok(Self {
            x,
            weights
        })
    }
}

impl<T: Num> Indicator for Wma<T> {
    type Input = T;
    type Output = T;
    type Error = NovaError;

    /// Adds an input value and returns its weighted average.
    ///
    /// During warmup, only the available inputs and corresponding newest
    /// weights are used.
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        self.x.push(input);
        let mut ws = T::zero();
        let mut xs = T::zero();
        for (i, w) in self.x.rev_iter().zip(self.weights.rev_iter()) {
            ws = math!(ws +(*w));
            xs = math!(xs + (*i) * (*w));
        }

        Ok(math!(xs / ws))
    }

    fn reset(&mut self) {
        self.x.clear();
    }
    
    fn ready(&self) -> bool {
        self.x.is_full()
    }
}

/// Linear Weighted Moving Average (LWMA).
///
/// A specialized WMA whose weights increase linearly from oldest to newest.
/// The oldest observation receives weight 1, and the newest receives weight
/// equal to the configured period.
///
/// # Calculation
///
/// For a period `n`:
///
/// ```text
/// LWMA = sum((i + 1) * input[i], i = 0..n-1)
///        / sum(i + 1, i = 0..n-1)
/// ```
///
/// Equivalently, the denominator is `n * (n + 1) / 2`.
///
/// # Warmup
///
/// The indicator uses the same partial-window behavior as [`Wma`]. It produces
/// outputs before the full period has been received.
pub struct LinearWma<T: Num> {
    wma: Wma<T>
}

impl<T: Num> LinearWma<T> {
    /// Creates a linear weighted moving average with the given period.
    ///
    /// The period must be greater than zero.
    pub fn new(period: usize) -> NovaResult<Self> {
        NovaError::period(period)?;
        let mut w = Window::new(period);
        (1..(period +1)).into_iter().for_each(|e| {
            w.push(T::from_usize(e));
        });
        let wma = Wma::new(w)?;
        Ok(Self {
            wma
        })
    }
}

impl<T: Num> Indicator for LinearWma<T> {
    type Input = T;
    type Output = T;
    type Error = NovaError;

    /// Adds a new value and returns the linear weighted moving average.    
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        self.wma.update(input)
    }

    fn reset(&mut self) {
        self.wma.reset();
    }

    fn ready(&self) -> bool {
        self.wma.ready()
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use nova_ta_core::Indicator;

    fn linear_wma(period: usize) -> LinearWma<f64> {
        LinearWma::new(period).expect("valid period")
    }

    fn approx_eq(actual: f64, expected: f64) {
        let tolerance = 1e-10;

        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    fn custom_wma(weights: &[f64]) -> Wma<f64> {
        let mut window = Window::new(weights.len());

        for &weight in weights {
            window.push(weight);
        }

        Wma::new(window).unwrap()
    }

    #[test]
    fn linear_wma_period_one_returns_input() {
        let mut indicator = linear_wma(1);

        approx_eq(indicator.update(10.0).unwrap(), 10.0);
        approx_eq(indicator.update(25.0).unwrap(), 25.0);

        assert!(indicator.ready());
    }

    #[test]
    fn linear_wma_calculates_weighted_average() {
        let mut indicator = linear_wma(3);

        indicator.update(10.0).unwrap();
        indicator.update(20.0).unwrap();

        let result = indicator.update(30.0).unwrap();

        // (10*1 + 20*2 + 30*3) / (1 + 2 + 3)
        approx_eq(result, 140.0 / 6.0);
    }

    #[test]
    fn newest_input_receives_greatest_linear_weight() {
        let mut indicator = linear_wma(3);

        indicator.update(0.0).unwrap();
        indicator.update(0.0).unwrap();

        let result = indicator.update(6.0).unwrap();

        // (0*1 + 0*2 + 6*3) / 6 = 3
        approx_eq(result, 3.0);
    }

    #[test]
    fn linear_wma_constant_input_returns_constant() {
        let mut indicator = linear_wma(4);

        for _ in 0..10 {
            approx_eq(indicator.update(7.5).unwrap(), 7.5);
        }
    }

    #[test]
    fn linear_wma_uses_partial_window_during_warmup() {
        let mut indicator = linear_wma(3);

        assert!(!indicator.ready());

        // First observation is paired with weight 3.
        approx_eq(indicator.update(10.0).unwrap(), 10.0);

        // The two newest weights are 3 and 2.
        // (20*3 + 10*2) / (3 + 2) = 16.
        approx_eq(indicator.update(20.0).unwrap(), 16.0);

        assert!(!indicator.ready());

        indicator.update(30.0).unwrap();
        assert!(indicator.ready());
    }

    #[test]
    fn linear_wma_updates_when_window_rolls() {
        let mut indicator = linear_wma(3);

        indicator.update(10.0).unwrap();
        indicator.update(20.0).unwrap();
        indicator.update(30.0).unwrap();

        // The window now contains [20, 30, 40].
        let result = indicator.update(40.0).unwrap();

        approx_eq(result, (20.0 + 60.0 + 120.0) / 6.0);
    }

    #[test]
    fn custom_wma_respects_supplied_weights() {
        let mut indicator = custom_wma(&[1.0, 2.0, 3.0]);

        indicator.update(10.0).unwrap();
        indicator.update(20.0).unwrap();

        let result = indicator.update(30.0).unwrap();

        // The newest input receives weight 3.
        approx_eq(result, (10.0 + 40.0 + 90.0) / 6.0);
    }

    #[test]
    fn custom_wma_constant_input_returns_constant() {
        let mut indicator = custom_wma(&[1.0, 2.0, 3.0]);

        for _ in 0..6 {
            approx_eq(indicator.update(12.0).unwrap(), 12.0);
        }
    }

    #[test]
    fn ready_becomes_true_when_window_is_full() {
        let mut indicator = linear_wma(3);

        assert!(!indicator.ready());

        indicator.update(1.0).unwrap();
        indicator.update(2.0).unwrap();
        assert!(!indicator.ready());

        indicator.update(3.0).unwrap();
        assert!(indicator.ready());
    }

    #[test]
    fn reset_clears_history() {
        let mut indicator = linear_wma(3);

        indicator.update(10.0).unwrap();
        indicator.update(20.0).unwrap();
        indicator.update(30.0).unwrap();

        assert!(indicator.ready());

        indicator.reset();

        assert!(!indicator.ready());
        approx_eq(indicator.update(42.0).unwrap(), 42.0);
        assert!(!indicator.ready());
    }

    #[test]
    fn rejects_zero_period() {
        assert!(LinearWma::<f64>::new(0).is_err());
    }
}
