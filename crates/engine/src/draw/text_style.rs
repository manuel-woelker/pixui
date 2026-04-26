use pixui_base::shared_string::SharedString;

/// Text style data referenced by draw styles.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// Font family used to render text.
    pub font_family: SharedString,
    /// Font size in draw units.
    pub font_size: f32,
}

impl TextStyle {
    /// Creates a text style.
    pub fn new(font_family: impl Into<SharedString>, font_size: f32) -> Self {
        Self {
            font_family: font_family.into(),
            font_size,
        }
    }
}
