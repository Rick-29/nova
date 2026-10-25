#[derive(Debug, thiserror::Error)]
pub enum NovaError {
    #[error(
        "This should never happend, if this message apears please contact Chipa Dev Team or Rick-29 on github."
    )]
    Never,
    #[error("Invalid period")]
    InvalidPeriod,
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Window has no element at index {0} (length: {1})")]
    WindowIdx(usize, usize),
    /// A numeric backend operation failed (overflow, division by zero, ...).
    /// Holds the `Debug` rendering of the backend's `Num::Error`.
    #[error("Numeric error: {0}")]
    Number(String),
}

impl NovaError {
    /// Wraps a [`nova_ta_core::Num::Error`] (or any `Debug` error) as [`NovaError::Number`].
    pub fn number<E: core::fmt::Debug>(err: E) -> Self {
        Self::Number(format!("{err:?}"))
    }

    pub fn period(period: usize) -> NovaResult<()> {
        if period == 0 {
            return Err(NovaError::InvalidPeriod);
        }
        Ok(())
    }

    pub fn never() -> Self {
        Self::Never
    }

    pub fn window(idx: usize, len: usize) -> Self {
        Self::WindowIdx(idx, len)
    }
}

pub type NovaResult<T> = Result<T, NovaError>;
