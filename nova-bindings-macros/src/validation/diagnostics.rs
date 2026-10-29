use std::fmt::Display;

use zyn::Span;
use zyn::ToTokens;

/// Collects every error found in one macro invocation so users see all
/// problems at once.
///
/// Errors are `syn::Error`s so each keeps its precise source span on stable
/// Rust.
#[derive(Default)]
pub struct Diagnostics {
    error: Option<syn::Error>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, error: syn::Error) {
        match &mut self.error {
            Some(existing) => existing.combine(error),
            None => self.error = Some(error),
        }
    }

    pub fn error(&mut self, span: Span, message: impl Display) {
        self.push(syn::Error::new(span, message));
    }

    pub fn error_spanned(&mut self, tokens: impl ToTokens, message: impl Display) {
        self.push(syn::Error::new_spanned(tokens, message));
    }

    /// Records the error of `result`, returning its value on success.
    pub fn take<T>(&mut self, result: syn::Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.push(error);
                None
            }
        }
    }

    pub fn finish(self) -> syn::Result<()> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Returns `value` when no error was recorded.
    pub fn finish_with<T>(self, value: T) -> syn::Result<T> {
        self.finish().map(|()| value)
    }
}

/// An error whose message ends with a `help:` line.
pub fn with_help(span: Span, message: impl Display, help: impl Display) -> syn::Error {
    syn::Error::new(span, format!("{message}\n\nhelp: {help}"))
}

/// Adds where an error occurred to its message.
pub trait WithContext {
    fn with_context(self, context: &str) -> syn::Error;
}

impl WithContext for syn::Error {
    fn with_context(self, context: &str) -> syn::Error {
        let message = self.to_string();
        let message = match message.split_once('\n') {
            Some((first, rest)) => format!("{first} in {context}\n{rest}"),
            None => format!("{message} in {context}"),
        };
        syn::Error::new(self.span(), message)
    }
}

/// Error for an unknown name, suggesting the closest known one.
pub fn unknown_name(span: Span, what: &str, found: &str, known: &[&str]) -> syn::Error {
    let help = match zyn::closest_match(found, known) {
        Some(candidate) => format!("did you mean `{candidate}`?"),
        None => format!("expected one of: {}", known.join(", ")),
    };
    with_help(span, format!("unknown {what} `{found}`"), help)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_all_errors() {
        let mut diags = Diagnostics::new();
        diags.error(Span::call_site(), "first");
        diags.error(Span::call_site(), "second");
        let messages: Vec<String> = diags
            .finish()
            .unwrap_err()
            .into_iter()
            .map(|error| error.to_string())
            .collect();
        assert_eq!(messages, ["first", "second"]);
    }

    #[test]
    fn suggests_close_names() {
        let error = unknown_name(
            Span::call_site(),
            "backend",
            "pyhton",
            &["python", "uniffi"],
        );
        assert!(error.to_string().contains("did you mean `python`?"));
    }

    #[test]
    fn lists_names_when_nothing_is_close() {
        let error = unknown_name(
            Span::call_site(),
            "backend",
            "kotlin",
            &["python", "uniffi"],
        );
        assert!(
            error
                .to_string()
                .contains("expected one of: python, uniffi")
        );
    }
}
