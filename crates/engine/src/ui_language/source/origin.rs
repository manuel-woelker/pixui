//! Source origins survive lowering and own the exact snapshot used to compile.
use super::{snapshot::Source, span::Span};
use pixui_base::{PixuiError, pixui_error};
#[derive(Clone, Debug)]
pub struct Origin {
    pub source: Source,
    pub span: Span,
}
impl Origin {
    pub fn error(&self, error: PixuiError) -> PixuiError {
        let (line, column) = self.source.position(self.span.start).unwrap_or((1, 1));
        pixui_error!("{}:{line}:{column}: {error}", self.source.filename)
    }
}
