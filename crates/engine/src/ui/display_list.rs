//! Owned, backend-independent drawing commands in painting order.

use super::{
    display_list_builder::ImageIndex,
    geometry::{Point, Rect},
    image::Image,
    instance::UiInstanceId,
    text::resource::{FontIndex, FontResource},
};
use pixui_base::{PixuiResult, pixui_error};
use std::sync::Arc;

/// Opaque sRGB color. The initial painter does not support transparency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    FillRect {
        rect: Rect,
        color: Color,
    },
    StrokeRect {
        rect: Rect,
        color: Color,
        width: f32,
    },
    /// Origin is the first baseline in logical pixels. Font indices are local.
    DrawText {
        origin: Point,
        text: String,
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    pub images: Vec<Image>,
    pub fonts: Vec<Arc<FontResource>>,
    pub commands: Vec<DrawCommand>,
}

impl DisplayList {
    /// Validates finite geometry, prepared glyphs, resource indices, and balanced clips.
    pub fn validate(&self) -> PixuiResult<()> {
        let mut depth = 0usize;
        for command in &self.commands {
            match command {
                DrawCommand::FillRect { rect, .. }
                | DrawCommand::StrokeRect { rect, .. }
                | DrawCommand::DrawImage {
                    destination: rect, ..
                }
                | DrawCommand::PushClip { rect }
                    if ![rect.x, rect.y, rect.width, rect.height]
                        .iter()
                        .all(|v| v.is_finite())
                        || rect.width < 0.0
                        || rect.height < 0.0 =>
                {
                    return Err(pixui_error!("invalid drawing rectangle"));
                }
                _ => {}
            }
            match command {
                DrawCommand::DrawImage { image, .. } if image.0 >= self.images.len() => {
                    return Err(pixui_error!("image index outside display list table"));
                }
                DrawCommand::DrawText { origin, .. }
                    if !origin.x.is_finite() || !origin.y.is_finite() =>
                {
                    return Err(pixui_error!("invalid text geometry"));
                }
                DrawCommand::DrawText { font, text, .. } => {
                    let resource = self
                        .fonts
                        .get(font.0)
                        .ok_or_else(|| pixui_error!("font index outside display list table"))?;
                    for character in text.chars() {
                        if character == '\n' {
                            continue;
                        }
                        if character.is_control() || resource.glyph(character).is_none() {
                            return Err(pixui_error!("text contains unprepared character"));
                        }
                    }
                }
                DrawCommand::PushClip { .. } => depth += 1,
                DrawCommand::PopClip => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| pixui_error!("unmatched PopClip"))?;
                }
                DrawCommand::StrokeRect { width, .. } if !width.is_finite() || *width <= 0.0 => {
                    return Err(pixui_error!("invalid stroke width"));
                }
                _ => {}
            }
        }
        if depth != 0 {
            return Err(pixui_error!("unclosed drawing clip"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct RenderRevision(pub u64);

/// Complete output for one instance. Replaces the previous output atomically.
/// Application borrows, hit regions, and native resources never cross this boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderOutput {
    pub instance_id: UiInstanceId,
    pub revision: RenderRevision,
    pub display_list: DisplayList,
    /// Optional scheduling request; no timer is started by headless consumers.
    pub redraw_after: Option<std::time::Duration>,
}
