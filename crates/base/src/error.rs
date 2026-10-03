//! Shared error and result types for pixui libraries.
//!
//! `PixuiError` is an [`error_stack::Report`] with a boxed dynamic error context,
//! and `PixuiResult` is a standard result whose error is a `PixuiError`. This gives
//! pixui APIs one error type while retaining error-stack reports and attachments.

use std::{error::Error, fmt};

use crate::PixuiString;

pub type PixuiError = error_stack::Report<BoxedError>;
pub type PixuiResult<T> = Result<T, PixuiError>;

/// An error context containing only a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageError {
    message: PixuiString,
}

impl MessageError {
    pub fn new(message: impl Into<PixuiString>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for MessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for MessageError {}

#[derive(Debug)]
pub struct BoxedError(Box<dyn Error + Send + Sync + 'static>);

impl BoxedError {
    pub fn new(error: impl Error + Send + Sync + 'static) -> Self {
        Self(Box::new(error))
    }

    pub fn downcast_ref<E: Error + 'static>(&self) -> Option<&E> {
        self.0.downcast_ref()
    }
}

impl fmt::Display for BoxedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for BoxedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

pub fn report(error: impl Error + Send + Sync + 'static) -> PixuiError {
    PixuiError::new(BoxedError::new(error))
}

pub fn message(message: impl Into<PixuiString>) -> PixuiError {
    report(MessageError::new(message))
}

/// Creates a [`PixuiError`] from a formatted message.
#[macro_export]
macro_rules! pixui_error {
    ($($argument:tt)*) => {
        $crate::message(::std::format!($($argument)*))
    };
}

/// Creates a [`PixuiResult`] error from a formatted message.
#[macro_export]
macro_rules! pixui_result {
    ($($argument:tt)*) => {
        ::core::result::Result::Err($crate::pixui_error!($($argument)*))
    };
}

/// Returns early with a formatted [`PixuiError`].
#[macro_export]
macro_rules! pixui_bail {
    ($($argument:tt)*) => {
        return $crate::pixui_result!($($argument)*)
    };
}

#[cfg(test)]
mod tests {
    use std::{error::Error, fmt};

    use super::{MessageError, PixuiResult, message, report};

    #[derive(Debug, PartialEq, Eq)]
    struct ExampleError;

    impl fmt::Display for ExampleError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("example error")
        }
    }

    impl Error for ExampleError {}

    fn fail() -> PixuiResult<()> {
        Err(report(ExampleError))
    }

    #[test]
    fn aliases_error_stack_report_and_result() {
        let error = fail().unwrap_err();

        assert_eq!(
            error.current_context().downcast_ref::<ExampleError>(),
            Some(&ExampleError)
        );
        assert_eq!(error.to_string(), "example error");
    }

    #[test]
    fn creates_message_errors() {
        let error = message("plain message");

        assert_eq!(error.to_string(), "plain message");
        assert_eq!(
            error.current_context().downcast_ref::<MessageError>(),
            Some(&MessageError::new("plain message"))
        );
    }

    #[test]
    fn formats_errors_and_results() {
        let error = crate::pixui_error!("invalid value `{}`", 42);
        let result: PixuiResult<()> = crate::pixui_result!("missing {}", "ticket");

        assert_eq!(error.to_string(), "invalid value `42`");
        assert_eq!(result.unwrap_err().to_string(), "missing ticket");
    }

    #[test]
    fn returns_early_with_a_formatted_error() {
        fn fail() -> PixuiResult<()> {
            crate::pixui_bail!("operation {}", "failed");
        }

        assert_eq!(fail().unwrap_err().to_string(), "operation failed");
    }
}
