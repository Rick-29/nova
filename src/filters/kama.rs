use nova_ta_core::{Indicator, Num};

use crate::{
    Ker,
    error::{NovaError, NovaResult},
    helpers::{add, mul, sub},
};

pub struct Kama<T: Num> {
    fast_sc: T,
    slow_sc: T,
    ker: Ker<T>,
    prev: Option<T>,
}

impl<T: Num> Kama<T> {
    pub fn new(fast: usize, slow: usize, ker: usize) -> NovaResult<Self> {
        NovaError::period(fast)?;
        NovaError::period(slow)?;
        if fast >= slow {
            return Err(NovaError::InvalidInput(format!(
                "Fast should be smaller then slow (got fast: {} and slow: {})",
                fast, slow
            )));
        }
        let ker = Ker::new(ker)?;
        let two = T::from_usize(2);
        let fast_sc = two
            .div(add(&T::from_usize(fast), &T::one())?)
            .map_err(NovaError::number)?;
        let slow_sc = two
            .div(add(&T::from_usize(slow), &T::one())?)
            .map_err(NovaError::number)?;
        Ok(Self {
            fast_sc,
            slow_sc,
            ker,
            prev: None,
        })
    }
}

impl<T: Num> Indicator for Kama<T> {
    type Input = T;
    type Output = T;
    type Error = NovaError;

    /// Processes a new price and returns the updated KAMA value.
    ///
    /// The first input initializes KAMA and returns the input unchanged.
    /// Later inputs update the ER and use it to determine the smoothing
    /// constant for the recursive moving-average calculation.
    ///
    /// The method returns a value before the ER window is full; check
    /// [`Indicator::ready`] if a fully warmed-up indicator is required.
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        if self.prev.is_none() {
            self.prev = Some(input);
            self.ker.update(input)?;
            return Ok(input);
        }
        let ker = self.ker.update(input)?;
        let sc_par = add(
            &mul(&ker, &sub(&self.fast_sc, &self.slow_sc)?)?,
            &self.slow_sc,
        )?;
        let sc = mul(&sc_par, &sc_par)?;
        let prev = self.prev.ok_or(NovaError::Never)?;
        let kama = add(&prev, &mul(&sc, &sub(&input, &prev)?)?)?;
        self.prev = Some(kama);
        Ok(kama)
    }

    /// Clears the previous KAMA value and resets the underlying ER indicator.
    ///
    /// After reset, the next update initializes KAMA from its input again.
    fn reset(&mut self) {
        self.prev = None;
        self.ker.reset();
    }

    /// Returns `true` when the ER indicator's lookback window is full and
    /// KAMA has been initialized.
    ///
    /// KAMA can produce outputs before this becomes `true`.
    fn ready(&self) -> bool {
        self.ker.ready() && self.prev.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_ta_core::Indicator;

    fn kama(fast: usize, slow: usize, er_period: usize) -> Kama<f64> {
        Kama::new(fast, slow, er_period).expect("valid KAMA configuration")
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
        let mut indicator = kama(2, 30, 10);

        let result = indicator.update(100.0).unwrap();

        approx_eq(result, 100.0);
    }

    #[test]
    fn rising_prices_move_kama_upward() {
        let mut indicator = kama(2, 30, 3);

        let first = indicator.update(10.0).unwrap();
        let second = indicator.update(11.0).unwrap();
        let third = indicator.update(12.0).unwrap();
        let fourth = indicator.update(13.0).unwrap();

        approx_eq(first, 10.0);
        assert!(second > first);
        assert!(third > second);
        assert!(fourth > third);

        // KAMA should move toward the current price without overshooting it.
        assert!(fourth <= 13.0);
    }

    #[test]
    fn falling_prices_move_kama_downward() {
        let mut indicator = kama(2, 30, 3);

        let first = indicator.update(13.0).unwrap();
        let second = indicator.update(12.0).unwrap();
        let third = indicator.update(11.0).unwrap();
        let fourth = indicator.update(10.0).unwrap();

        approx_eq(first, 13.0);
        assert!(second < first);
        assert!(third < second);
        assert!(fourth < third);
        assert!(fourth >= 10.0);
    }

    #[test]
    fn flat_prices_leave_kama_unchanged() {
        let mut indicator = kama(2, 30, 3);

        indicator.update(5.0).unwrap();
        let second = indicator.update(5.0).unwrap();
        let third = indicator.update(5.0).unwrap();
        let fourth = indicator.update(5.0).unwrap();

        approx_eq(second, 5.0);
        approx_eq(third, 5.0);
        approx_eq(fourth, 5.0);
    }

    #[test]
    fn ready_becomes_true_when_er_window_is_full() {
        let mut indicator = kama(2, 30, 3);

        assert!(!indicator.ready());

        indicator.update(10.0).unwrap();
        indicator.update(11.0).unwrap();
        indicator.update(12.0).unwrap();

        // The ER window has capacity 3 and should now be full.
        assert!(indicator.ready());
    }

    #[test]
    fn reset_clears_indicator_state() {
        let mut indicator = kama(2, 30, 3);

        indicator.update(10.0).unwrap();
        indicator.update(12.0).unwrap();
        indicator.update(14.0).unwrap();

        indicator.reset();

        assert!(!indicator.ready());

        // The next input should seed KAMA from scratch.
        let result = indicator.update(50.0).unwrap();
        approx_eq(result, 50.0);
        assert!(!indicator.ready());
    }

    #[test]
    fn rejects_zero_fast_period() {
        assert!(Kama::<f64>::new(0, 30, 10).is_err());
    }

    #[test]
    fn rejects_zero_slow_period() {
        assert!(Kama::<f64>::new(2, 0, 10).is_err());
    }

    #[test]
    fn rejects_fast_period_equal_to_slow_period() {
        assert!(Kama::<f64>::new(10, 10, 10).is_err());
    }

    #[test]
    fn rejects_fast_period_greater_than_slow_period() {
        assert!(Kama::<f64>::new(30, 2, 10).is_err());
    }

    #[test]
    fn rejects_zero_er_period() {
        assert!(Kama::<f64>::new(2, 30, 0).is_err());
    }

    #[test]
    fn follows_standard_kama_formula() {
        let mut indicator = kama(2, 30, 3);

        // First input initializes KAMA.
        approx_eq(indicator.update(10.0).unwrap(), 10.0);

        // Second input: ER = 1 / 1 = 1.
        // fast_sc = 2 / (2 + 1) = 2/3
        // slow_sc = 2 / (30 + 1) = 2/31
        // SC = (1 * (2/3 - 2/31) + 2/31)^2 = (2/3)^2
        // KAMA = 10 + (4/9) * (11 - 10)
        let result = indicator.update(11.0).unwrap();

        approx_eq(result, 10.0 + 4.0 / 9.0);
    }
}
