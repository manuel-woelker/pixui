use crate::ui_language::source::span::Span;
#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Name(String),
    Number(String),
    String { translated: bool, value: String },
    Less,
    Greater,
    Slash,
    Equal,
    EqualEqual,
    NotEqual,
    Bang,
    And,
    Or,
    Dot,
    Comma,
    Semicolon,
    Colon,
    At,
    Arrow,
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    Eof,
}
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}
