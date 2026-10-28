#![allow(unused)]
use nova_ta_core::{Num, Window};

use crate::error::{NovaError, NovaResult};

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
