//! Checked nodes carry concrete descriptors and resolved field indices.
use crate::ui_language::{
    ast::{definition::Definition, expression::Operator},
    resolution::{
        environment::{ComponentExport, ReferenceExport, Snapshot, ValueType},
        resolved::Root,
    },
    source::span::Span,
};
use crate::{
    application::action_handle::ActionHandle,
    layout::{container::ContainerPart, style::LayoutStyle},
    styles::slots::SlotAssignment,
};
use pixui_reflect::{FieldIndex, TypeDescriptor};
#[derive(Clone)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub descriptor: &'static TypeDescriptor,
    pub span: Span,
}
#[derive(Clone)]
pub enum ExpressionKind {
    Constant {
        value: Snapshot,
        ty: ValueType,
    },
    Path {
        root: Root,
        capture_type: Option<ValueType>,
        capture_result: Option<ValueType>,
        fields: Vec<(FieldIndex, &'static TypeDescriptor)>,
    },
    Template {
        translated: bool,
        source: String,
        arguments: Vec<(String, Expression)>,
    },
    ResourcePath(Box<Expression>),
    LocalizedText(Box<Expression>),
    Not(Box<Expression>),
    Binary {
        op: Operator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
}
pub struct TypedDefinition {
    pub syntax: Definition,
    pub root: TypedPart,
}
pub struct TypedPart {
    pub span: Span,
    pub kind: PartKind,
}
pub enum PartKind {
    Dynamic(Expression),
    Component {
        export: ComponentExport,
        props: Vec<Expression>,
        types: Vec<ValueType>,
        callbacks: Vec<(String, Action)>,
        layout: LayoutStyle,
        styles: Vec<SlotAssignment>,
        focus: crate::ui::focus::FocusBehavior,
    },
    Composite(Vec<TypedPart>),
    Container {
        container: ContainerPart,
        children: Vec<TypedPart>,
    },
    For {
        sequence: Expression,
        body: Box<TypedPart>,
    },
    Match {
        value: Expression,
        arms: Vec<(Option<Expression>, TypedPart)>,
    },
}
#[derive(Clone)]
pub struct Action {
    pub handle: ActionHandle,
    pub arguments: Vec<ActionArgument>,
    pub span: Span,
}
pub use crate::events::program::ReferenceRoot;
#[derive(Clone)]
pub enum ActionArgument {
    Value(Expression, ValueType),
    Reference(ReferenceRoot, ReferenceExport),
}
