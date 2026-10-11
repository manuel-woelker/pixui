//! Borrowed renderer input. Indices resolve without cloning command text.
use super::{
    display_list::{Color, DisplayList, DrawCommand, RenderOutput},
    display_list_builder::ImageIndex,
    geometry::{Point, Rect},
    text::resource::FontIndex,
};
use crate::styles::{index::StyleIndex, style::Border};
use pixui_base::{PixuiResult, pixui_error};
#[derive(Clone, Debug, PartialEq)]
pub struct RenderStyle {
    pub fill: Option<Color>,
    pub border: Option<Border>,
    pub foreground: Color,
    pub font: Option<FontIndex>,
}
type StyleKey = (
    Option<Color>,
    Option<(Color, u32)>,
    Color,
    Option<FontIndex>,
);
impl RenderStyle {
    pub(crate) fn key(&self) -> StyleKey {
        (
            self.fill,
            self.border
                .map(|border| (border.color, border.width.to_bits())),
            self.foreground,
            self.font,
        )
    }
}
/// The styles and resources must come from the same completed output.
#[derive(Clone, Copy)]
pub struct RenderInput<'a> {
    pub display: &'a DisplayList,
    pub styles: &'a [RenderStyle],
}
impl<'a> From<&'a DisplayList> for RenderInput<'a> {
    fn from(display: &'a DisplayList) -> Self {
        Self {
            display,
            styles: &[],
        }
    }
}
impl<'a> From<&'a RenderOutput> for RenderInput<'a> {
    fn from(output: &'a RenderOutput) -> Self {
        Self {
            display: &output.display_list,
            styles: &output.styles,
        }
    }
}
/// Scalar values are copied; text and resource snapshots remain borrowed.
pub enum ResolvedCommand<'a> {
    FillRect {
        rect: Rect,
        color: Color,
    },
    StrokeRect {
        rect: Rect,
        color: Color,
        width: f32,
    },
    DrawText {
        origin: Point,
        text: &'a str,
        font: FontIndex,
        color: Color,
    },
    DrawImage {
        image: ImageIndex,
        destination: Rect,
    },
    PushClip {
        rect: Rect,
    },
    PopClip,
}
impl<'a> RenderInput<'a> {
    pub fn style(&self, index: StyleIndex) -> PixuiResult<&'a RenderStyle> {
        self.styles
            .get(index.index)
            .ok_or_else(|| pixui_error!("style index outside render output table"))
    }
    pub fn validate(&self) -> PixuiResult<()> {
        for style in self.styles {
            if style
                .border
                .is_some_and(|border| !border.width.is_finite() || border.width <= 0.0)
            {
                return Err(pixui_error!("invalid render style border"));
            }
            if style
                .font
                .is_some_and(|font| self.display.fonts.get(font).is_none())
            {
                return Err(pixui_error!("style font outside display resource table"));
            }
        }
        self.display.validate_with_styles(self.styles)
    }
    pub fn resolve(&self, command: &'a DrawCommand) -> PixuiResult<ResolvedCommand<'a>> {
        Ok(match command {
            DrawCommand::FillRect { rect, color } => ResolvedCommand::FillRect {
                rect: *rect,
                color: *color,
            },
            DrawCommand::StrokeRect { rect, color, width } => ResolvedCommand::StrokeRect {
                rect: *rect,
                color: *color,
                width: *width,
            },
            DrawCommand::DrawText {
                origin,
                text,
                font,
                color,
            } => ResolvedCommand::DrawText {
                origin: *origin,
                text,
                font: *font,
                color: *color,
            },
            DrawCommand::StyledFillRect { rect, style } => ResolvedCommand::FillRect {
                rect: *rect,
                color: self
                    .style(*style)?
                    .fill
                    .ok_or_else(|| pixui_error!("style has no fill"))?,
            },
            DrawCommand::StyledStrokeRect { rect, style } => {
                let border = self
                    .style(*style)?
                    .border
                    .ok_or_else(|| pixui_error!("style has no border"))?;
                ResolvedCommand::StrokeRect {
                    rect: *rect,
                    color: border.color,
                    width: border.width,
                }
            }
            DrawCommand::StyledText {
                origin,
                text,
                style,
            } => {
                let style = self.style(*style)?;
                ResolvedCommand::DrawText {
                    origin: *origin,
                    text,
                    font: style
                        .font
                        .ok_or_else(|| pixui_error!("style has no prepared font"))?,
                    color: style.foreground,
                }
            }
            DrawCommand::DrawImage { image, destination } => ResolvedCommand::DrawImage {
                image: *image,
                destination: *destination,
            },
            DrawCommand::PushClip { rect } => ResolvedCommand::PushClip { rect: *rect },
            DrawCommand::PopClip => ResolvedCommand::PopClip,
        })
    }
}

/// Headless rendering owns both tables. Deref exposes display resources/commands;
/// pass the whole value to a renderer to retain its styles.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderedDisplay {
    pub display_list: DisplayList,
    pub styles: Vec<RenderStyle>,
}
impl std::ops::Deref for RenderedDisplay {
    type Target = DisplayList;
    fn deref(&self) -> &DisplayList {
        &self.display_list
    }
}
impl<'a> From<&'a RenderedDisplay> for RenderInput<'a> {
    fn from(output: &'a RenderedDisplay) -> Self {
        Self {
            display: &output.display_list,
            styles: &output.styles,
        }
    }
}
impl RenderedDisplay {
    pub fn validate(&self) -> PixuiResult<()> {
        RenderInput::from(self).validate()
    }
}
