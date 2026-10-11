use super::expression::{Expression, Name};
use crate::ui_language::source::span::Span;
#[derive(Clone, Debug)]
pub struct Part {
    pub kind: PartKind,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub enum PartKind {
    Element {
        name: Option<Name>,
        attributes: Vec<Attribute>,
        children: Vec<Part>,
    },
    For {
        binding: Name,
        sequence: Expression,
        body: Box<Part>,
    },
    Match {
        value: Expression,
        arms: Vec<Arm>,
    },
}
#[derive(Clone, Debug)]
pub struct Attribute {
    pub name: Name,
    pub value: Expression,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct Arm {
    pub pattern: Option<Expression>,
    pub body: Part,
    pub span: Span,
}
