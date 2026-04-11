use pixui_base::shared_string::SharedString;
use pixui_base::span::Span;

/// Parsed string property on a UI element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiProperty {
    /// Property name as written in the source.
    pub name: SharedString,
    /// Decoded string value.
    pub value: SharedString,
    /// Full source span covering the property.
    pub span: Span,
}
