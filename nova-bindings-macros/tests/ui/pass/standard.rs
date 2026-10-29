#![allow(unexpected_cfgs, dead_code)]

use nova_bindings_macros::{Bindings, bindings, bindings_error};

pub trait Indicator {
    type Input;
    type Output;
    type Error;
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, Self::Error>;
    fn reset(&mut self);
    fn ready(&self) -> bool;
}

#[bindings_error]
#[derive(Debug)]
pub enum MyError {
    InvalidPeriod,
}

impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid period")
    }
}

/// Simple moving average.
#[derive(Bindings)]
#[bindings(error = MyError, python(update = both, module = "fixtures"))]
pub struct Sma<T> {
    period: usize,
    last: Option<T>,
}

#[bindings]
impl<T> Sma<T> {
    /// Creates it.
    pub fn new(period: usize) -> Result<Self, MyError> {
        if period == 0 {
            return Err(MyError::InvalidPeriod);
        }
        Ok(Self { period, last: None })
    }

    #[bindings(export, name = "length")]
    pub fn period(&self) -> usize {
        self.period
    }
}

#[bindings]
impl<T: Copy> Indicator for Sma<T> {
    type Input = T;
    type Output = T;
    type Error = MyError;
    fn update(&mut self, input: T) -> Result<T, MyError> {
        Ok(input)
    }
    fn reset(&mut self) {}
    fn ready(&self) -> bool {
        true
    }
}

fn main() {
    let mut sma = Sma::<f64>::new(3).unwrap();
    assert_eq!(sma.update(2.0).unwrap(), 2.0);
    assert_eq!(sma.period(), 3);
}
