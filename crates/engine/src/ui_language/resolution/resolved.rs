//! Names resolved against explicit exports; field types are checked next.
use super::environment::{ComponentExport, DataExport};
use crate::application::action_handle::ActionHandle;
use crate::ui_language::{
    ast::{
        definition::Definition,
        expression::{Name, Operator},
        part::Part,
    },
    source::span::Span,
};
#[derive(Clone)]
pub enum Root {
    Layout(Box<crate::layout::style::LayoutStyle>),
    Styles(Vec<crate::styles::slots::SlotAssignment>),
    Data(Box<DataExport>),
    Local(usize),
    Event(&'static pixui_reflect::TypeDescriptor),
}
#[derive(Clone)]
pub struct Expression {
    pub spelling: Option<String>,
    pub kind: ExpressionKind,
    pub span: Span,
}
#[derive(Clone)]
pub enum ExpressionKind {
    Bool(bool),
    Number(String),
    Template {
        translated: bool,
        segments: Vec<Segment>,
    },
    Path {
        root: Root,
        fields: Vec<Name>,
    },
    Call {
        handle: ActionHandle,
        arguments: Vec<(Name, Expression)>,
    },
    Not(Box<Expression>),
    Binary {
        op: Operator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
}
#[derive(Clone)]
pub enum Segment {
    Text(String, Span),
    Value(Expression),
}
pub struct ResolvedDefinition {
    pub syntax: Definition,
    pub root: ResolvedPart,
}
pub struct ResolvedPart {
    pub syntax: Part,
    pub kind: PartKind,
}
pub enum PartKind {
    Element {
        component: Option<ComponentExport>,
        attributes: Vec<(Name, Expression)>,
        children: Vec<ResolvedPart>,
    },
    For {
        sequence: Expression,
        body: Box<ResolvedPart>,
    },
    Match {
        value: Expression,
        arms: Vec<(Option<Expression>, ResolvedPart, Span)>,
    },
}
