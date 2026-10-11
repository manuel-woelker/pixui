//! Shared structured diagnostics for every compiler stage.
use crate::ui_language::source::span::Span;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Lexer,
    Parser,
    Resolution,
    Checking,
    Lowering,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}
#[derive(Clone, Debug)]
pub struct Label {
    pub span: Span,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub stage: Stage,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub primary: Span,
    pub secondary: Vec<Label>,
    pub notes: Vec<String>,
}
impl Diagnostic {
    pub fn new(stage: Stage, code: &'static str, span: Span, message: impl Into<String>) -> Self {
        Self {
            stage,
            severity: Severity::Error,
            code,
            message: message.into(),
            primary: span,
            secondary: vec![],
            notes: vec![],
        }
    }
    pub fn secondary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.secondary.push(Label {
            span,
            message: message.into(),
        });
        self
    }
}
