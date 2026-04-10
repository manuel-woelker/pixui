use crate::shared_string::SharedString;
use crate::span::Span;

/// Message attached to one highlighted source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceAnnotation {
    /// Byte span highlighted by this annotation.
    pub span: Span,
    /// User-facing annotation message.
    pub message: SharedString,
}

impl SourceAnnotation {
    /// Creates a source annotation from its span and message.
    pub fn new(span: Span, message: impl Into<SharedString>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}
