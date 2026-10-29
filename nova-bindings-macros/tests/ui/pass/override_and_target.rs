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

pub mod model {
    #[derive(nova_bindings_macros::Bindings)]
    #[bindings(error = super::MyError, backends(python, uniffi), overrides(update(python)))]
    pub struct Sma<T> {
        pub(crate) last: Option<T>,
    }
}

use model::Sma;

#[bindings(target = model::Sma)]
impl<T> Sma<T> {
    #[bindings(constructor)]
    pub fn empty() -> Self {
        Self { last: None }
    }
}
#[bindings(target = model::Sma)]
impl<T: Copy> Indicator for Sma<T> {
    type Input = T;
    type Output = T;
    type Error = MyError;
    fn update(&mut self, input: T) -> Result<T, MyError> {
        self.last = Some(input);
        Ok(input)
    }
    fn reset(&mut self) {}
    fn ready(&self) -> bool {
        true
    }
}

#[bindings(method = update, target = model::Sma)]
impl Sma<f64> {
    pub fn update_batch(&mut self, values: Vec<f64>) -> Result<Vec<f64>, MyError> {
        values.into_iter().map(|value| self.update(value)).collect()
    }
}

fn main() {
    assert_eq!(Sma::<f64>::empty().update_batch(vec![1.0]).unwrap(), vec![1.0]);
}
