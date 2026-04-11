use crate::ui_description::token_kind::TokenKind;
use pixui_base::span::Span;

/// Single token with its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}
