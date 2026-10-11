//! Concrete worker styles and sparse overrides. Outer layout stays in LayoutStyle.
use super::theme::{ThemeFont, ThemeTokens};
use crate::ui::{display_list::Color, text::font::FontConfig};
use pixui_base::{PixuiResult, pixui_error};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub color: Color,
    pub width: f32,
}
#[derive(Clone)]
pub struct PaintStyle {
    pub fill: Option<Color>,
    pub border: Option<Border>,
    pub foreground: Color,
    pub font: ThemeFont,
    pub font_size: f32,
    pub inset: f32,
    pub minimum_height: f32,
}
impl PaintStyle {
    pub fn text(tokens: &ThemeTokens) -> Self {
        Self {
            fill: None,
            border: None,
            foreground: tokens.foreground,
            font: tokens.font.clone(),
            font_size: tokens.font_size,
            inset: tokens.inset,
            minimum_height: tokens.minimum_height,
        }
    }
    pub fn font_config(&self, scale: f32) -> PixuiResult<FontConfig> {
        FontConfig::new(self.font.face()?, self.font_size, scale)
    }
    pub fn validate(&self, scale: f32) -> PixuiResult<()> {
        if !self.inset.is_finite()
            || self.inset < 0.0
            || !self.minimum_height.is_finite()
            || self.minimum_height < 0.0
            || self
                .border
                .is_some_and(|border| !border.width.is_finite() || border.width <= 0.0)
        {
            return Err(pixui_error!("invalid style metrics"));
        }
        self.font_config(scale)?;
        Ok(())
    }
}
/// None leaves a field unchanged. Some(None) explicitly removes fill/border.
#[derive(Clone, Default)]
pub struct StylePatch {
    pub fill: Option<Option<Color>>,
    pub border: Option<Option<Border>>,
    pub foreground: Option<Color>,
    pub font: Option<ThemeFont>,
    pub font_size: Option<f32>,
    pub inset: Option<f32>,
    pub minimum_height: Option<f32>,
}
impl StylePatch {
    pub fn apply(&self, style: &mut PaintStyle) {
        if let Some(value) = self.fill {
            style.fill = value;
        }
        if let Some(value) = self.border {
            style.border = value;
        }
        if let Some(value) = self.foreground {
            style.foreground = value;
        }
        if let Some(value) = &self.font {
            style.font = value.clone();
        }
        if let Some(value) = self.font_size {
            style.font_size = value;
        }
        if let Some(value) = self.inset {
            style.inset = value;
        }
        if let Some(value) = self.minimum_height {
            style.minimum_height = value;
        }
    }
}
/// Interaction patches cannot change metrics or add/remove a border. This keeps
/// measurement valid when hover is recomputed after layout.
#[derive(Clone, Copy, Default)]
pub struct StatePatch {
    pub fill: Option<Color>,
    pub foreground: Option<Color>,
    pub border: Option<Color>,
}
impl StatePatch {
    pub fn apply(&self, style: &mut PaintStyle) {
        if let Some(value) = self.fill {
            style.fill = Some(value);
        }
        if let Some(value) = self.foreground {
            style.foreground = value;
        }
        if let Some(value) = self.border
            && let Some(border) = &mut style.border
        {
            border.color = value;
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct StyleStates {
    pub active: bool,
    pub hovered: bool,
    pub disabled: bool,
}
