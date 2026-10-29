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

pub struct Candle<T>(T);

#[derive(Bindings)]
#[bindings(error = MyError)]
pub struct Sma<T> {
    last: Option<T>,
}

#[bindings]
impl<T> Sma<T> {
    pub fn new() -> Self {
        Self { last: None }
    }
}

#[bindings]
impl<T: Copy> Indicator for Sma<T> {
    type Input = Candle<T>;
    type Output = Vec<T>;
    type Error = MyError;
    fn update(&mut self, input: Self::Input) -> Result<Self::Output, MyError> {
        Ok(vec![input.0])
    }
    fn reset(&mut self) {}
    fn ready(&self) -> bool {
        true
    }
}

fn main() {}
