use nova_ta_core::{Indicator, Ohlc, Real, Window};

use crate::error::{NovaError, NovaResult};


pub struct Alma<T: Real> {
    candles: Window<Ohlc<T>>,
    weights: Window<f64>,
}

impl<T: Real> Alma<T> {
    pub fn new(period: usize, offset: f64, sigma: u32) -> NovaResult<Self> {
        if period == 0 {
            return Err(NovaError::InvalidPeriod)
        }
        if !(0.0..=1.0).contains(&offset) {
            return Err(NovaError::InvalidInput(format!("offset, value not between 0.0 and 1.0 (got `{}`)", offset)))
        }
        if sigma == 0 {
            return Err(NovaError::InvalidInput("Invalid value for sigma".to_string()))
        }
        let candles: Window<Ohlc<T>> = Window::new(period);
        let mut weights = Window::new(period);
        let m = (offset * (period - 1) as f64).floor() as usize;
        let s = period as f64 / sigma as f64;
        for i in 0..period {
            let d = i as f64 - m as f64;
            weights.push((-(d * d) / (2.0 * s * s)).exp());
        }
        Ok(
            Self { candles, weights }
        )
    }
}

impl<T: Real> Indicator for Alma<T> {
    type Input = Ohlc<T>;
    type Output = T;
    type Error = NovaError;

    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        self.candles.push(input);
        // Newest candle pairs with the newest weight; during warm-up only the
        // newest `candles.len()` weights are used, so the output is a
        // properly normalised partial result.
        let mut sum = T::zero();
        let mut weight_sum = T::zero();
        for (candle, w) in self.candles.rev_iter().zip(self.weights.rev_iter()) {
            let w = T::from_f64(*w).map_err(NovaError::number)?;
            sum = sum
                .add(candle.close.mul(w).map_err(NovaError::number)?)
                .map_err(NovaError::number)?;
            weight_sum = weight_sum.add(w).map_err(NovaError::number)?;
        }
        sum.div(weight_sum).map_err(NovaError::number)
    }

    fn ready(&self) -> bool {
        self.candles.is_full()
    }

    fn reset(&mut self) {
        // Weights don't reset
        self.candles.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn bar(c: f64) -> Ohlc<f64> {
        Ohlc::prices(c, c, c, c)
    }

    #[test]
    fn constant_input_gives_constant_output_even_in_warmup() {
        let mut alma = Alma::<f64>::new(5, 0.85, 6).unwrap();
        for i in 0..8 {
            let v = alma.update(bar(10.0)).unwrap();
            assert!((v - 10.0).abs() < 1e-12);
            assert_eq!(alma.ready(), i >= 4);
        }
    }

    #[test]
    fn matches_hand_computed_value() {
        // period 3, offset 1.0 -> m = 2; sigma 3 -> s = 1; weights e^-2, e^-0.5, 1.
        let mut alma = Alma::<f64>::new(3, 1.0, 3).unwrap();
        let (w0, w1, w2) = ((-2.0_f64).exp(), (-0.5_f64).exp(), 1.0);
        alma.update(bar(1.0)).unwrap();
        // Warm-up: only the newest weights (w1, w2) are used.
        let v = alma.update(bar(2.0)).unwrap();
        assert!((v - (1.0 * w1 + 2.0 * w2) / (w1 + w2)).abs() < 1e-12);
        let v = alma.update(bar(4.0)).unwrap();
        assert!((v - (1.0 * w0 + 2.0 * w1 + 4.0 * w2) / (w0 + w1 + w2)).abs() < 1e-12);
    }

    #[test]
    fn reset_restarts_warmup() {
        let mut alma = Alma::<f64>::new(2, 0.5, 6).unwrap();
        alma.update(bar(1.0)).unwrap();
        alma.update(bar(2.0)).unwrap();
        assert!(alma.ready());
        alma.reset();
        assert!(!alma.ready());
        assert_eq!(alma.update(bar(7.0)).unwrap(), 7.0);
    }

    #[test]
    fn rejects_bad_parameters() {
        assert!(Alma::<f64>::new(0, 0.5, 6).is_err());
        assert!(Alma::<f64>::new(3, 1.5, 6).is_err());
        assert!(Alma::<f64>::new(3, f64::NAN, 6).is_err());
        assert!(Alma::<f64>::new(3, 0.5, 0).is_err());
    }
}
