#![allow(unused)]
use nova_ta_core::{Num, Window, math_support};

use crate::error::{NovaError, NovaResult};

math_support!(math, NovaError::number, T);

pub(crate) fn sum<T: Num>(w: &Window<T>) -> NovaResult<T> {
    let mut res = T::zero();
    for e in w.iter() {
        res = math!(res + (*e));
    }
    Ok(res)
}

pub(crate) fn w_sum<T: Num>(bias: &Window<T>, weights: &Window<T>) -> NovaResult<T> {
    let mut res = T::zero();
    for (b ,w) in bias.iter().zip(weights.iter()) {
        res = math!(res + (*b) * (*w));
    }
    Ok(res)
}

pub(crate) fn mul<T: Num>(l: &T, r: &T) -> NovaResult<T> {
    l.mul(*r).map_err(NovaError::number)
}

pub(crate) fn div<T: Num>(l: &T, r: &T) -> NovaResult<T> {
    l.div(*r).map_err(NovaError::number)
}

pub(crate) fn add<T: Num>(l: &T, r: &T) -> NovaResult<T> {
    l.add(*r).map_err(NovaError::number)
}

pub(crate) fn sub<T: Num>(l: &T, r: &T) -> NovaResult<T> {
    l.sub(*r).map_err(NovaError::number)
}
#[allow(unused)]
pub(crate) fn get<T>(window: &Window<T>, idx: usize) -> NovaResult<&T> {
    window
        .get(idx)
        .ok_or(NovaError::WindowIdx(idx, window.len()))
}
