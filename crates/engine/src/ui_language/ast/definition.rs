use super::{expression::Name, part::Part};
use crate::ui_language::source::span::Span;
#[derive(Clone, Debug)]
pub struct Definition {
    pub name: Name,
    pub domain: Option<Name>,
    pub root: Part,
    pub span: Span,
}
