use pixui_base::shared_string::SharedString;

/// Token kinds produced by the UI description lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    LessThan,
    GreaterThan,
    LessThanSlash,
    SlashGreaterThan,
    Equals,
    Identifier(SharedString),
    StringLiteral(SharedString),
}
