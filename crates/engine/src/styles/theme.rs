//! Semantic application colors and metrics; independent of individual painters.
use crate::ui::{display_list::Color, presentation::Theme, text::font::FontFace};
use pixui_base::PixuiResult;

/// Font selection is worker-side; renderer outputs retain prepared snapshots.
#[derive(Clone)]
pub enum ThemeFont {
    Regular,
    Mono,
    Face(FontFace),
}
impl ThemeFont {
    pub fn face(&self) -> PixuiResult<FontFace> {
        match self {
            Self::Regular => FontFace::geist(),
            Self::Mono => FontFace::geist_mono(),
            Self::Face(face) => Ok(face.clone()),
        }
    }
}
#[derive(Clone)]
pub struct ThemeTokens {
    pub canvas: Color,
    pub surface: Color,
    pub foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub border: Color,
    pub selection: Color,
    pub danger: Color,
    pub font: ThemeFont,
    pub font_size: f32,
    pub inset: f32,
    pub minimum_height: f32,
    pub stroke_width: f32,
}
impl ThemeTokens {
    pub fn for_theme(theme: Theme) -> Self {
        let palette = crate::painters::palette::Palette::for_theme(theme);
        Self {
            canvas: palette.background,
            surface: palette.control,
            foreground: palette.foreground,
            muted: Color(125, 125, 135),
            accent: palette.accent,
            on_accent: if theme == Theme::Dark {
                palette.background
            } else {
                Color(255, 255, 255)
            },
            border: palette.foreground,
            selection: if theme == Theme::Dark {
                Color(55, 65, 80)
            } else {
                Color(180, 205, 245)
            },
            danger: Color(190, 45, 55),
            font: ThemeFont::Regular,
            font_size: 16.0,
            inset: 8.0,
            minimum_height: 36.0,
            stroke_width: 2.0,
        }
    }
    pub(crate) fn validate(&self) -> PixuiResult<()> {
        if !self.stroke_width.is_finite() || self.stroke_width <= 0.0 {
            return Err(pixui_base::pixui_error!("invalid theme stroke width"));
        }
        crate::styles::style::PaintStyle::text(self).validate(1.0)?;
        Ok(())
    }
}
