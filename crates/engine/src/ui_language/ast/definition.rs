use super::{expression::Name, part::Part};
use crate::ui_language::source::span::Span;
#[derive(Clone, Debug)]
pub struct Definition {
    pub imports: Vec<Import>,
    pub name: Name,
    pub domain: Option<Name>,
    pub root: Part,
    pub span: Span,
}

/// An import alias and source specifier retain independent diagnostic spans.
#[derive(Clone, Debug)]
pub struct Import {
    pub members: Vec<ImportMember>,
    pub alias: Name,
    pub path: Name,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Module {
    pub imports: Vec<Import>,
    pub components: Vec<Component>,
    pub entry: Option<Definition>,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct Component {
    pub name: Name,
    pub parameters: Vec<Parameter>,
    pub body: Part,
    pub export: Option<Span>,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: Name,
    pub ty: Name,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub struct ImportMember {
    pub name: Name,
    pub alias: Name,
    pub span: Span,
}
