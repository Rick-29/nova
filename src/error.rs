
#[derive(Debug, thiserror::Error)]
pub enum NovaError {
    #[error("Invalid period")]
    InvalidPeriod,
    #[error("Invalid input: {0}")]
    InvalidInput(String),
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
}

pub type NovaResult<T> = Result<T, NovaError>;
