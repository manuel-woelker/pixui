use crate::ui_language::source::span::Span;
#[derive(Clone, Debug)]
pub struct Name {
    pub value: String,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub enum ExpressionKind {
    Bool(bool),
    Number(String),
    Template {
        translated: bool,
        segments: Vec<Segment>,
    },
    Path(Vec<Name>),
    Call {
        path: Vec<Name>,
        arguments: Vec<Expression>,
    },
    Not(Box<Expression>),
    Binary {
        op: Operator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
}
#[derive(Clone, Debug)]
pub struct Segment {
    pub span: Span,
    pub kind: SegmentKind,
}
#[derive(Clone, Debug)]
pub enum SegmentKind {
    Text(String),
    Value(Expression),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    Equal,
    NotEqual,
    And,
    Or,
}
