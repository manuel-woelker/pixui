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
    pub alias: Name,
    pub path: Name,
    pub span: Span,
}
