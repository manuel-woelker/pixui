use crate::ui_description::ast::ui_property::UiProperty;
use pixui_base::shared_string::SharedString;
use pixui_base::span::Span;

/// Parsed UI element node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiElement {
    /// Tag name as written in the source.
    pub tag_name: SharedString,
    /// Ordered string properties for the element.
    pub properties: Vec<UiProperty>,
    /// Ordered child elements.
    pub children: Vec<UiElement>,
    /// Full source span covering the element.
    pub span: Span,
}
