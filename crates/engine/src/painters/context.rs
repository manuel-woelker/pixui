//! Typed, borrowed input and a local drawing-command sink for one component.

use crate::{
    live_model::component::Component,
    ui::{
        display_list::{Color, DrawCommand},
        display_list_builder::DisplayListBuilder,
        geometry::{Point, Rect, Size},
        image::Image,
        presentation::PresentationSettings,
        text::{
            font::{FontConfig, FontFace},
            resource::FontMetrics,
        },
    },
};
use pixui_base::PixuiResult;

pub struct PaintContext<'a, C: Component> {
    pub props: &'a C::Props,
    pub state: &'a C::State,
    pub width: f32,
    pub height: f32,
    pub settings: &'a PresentationSettings,
    pub focused: bool,
    pub hovered: bool,
    /// Master timeline time in microseconds, identical for every painter in this
    /// render. Use this instead of reading the system clock; overrides can seek.
    pub timestamp_us: u64,
    pub(crate) display: &'a mut DisplayListBuilder,
    pub(crate) origin: Point,
    pub(crate) clip_depth: usize,
    pub(crate) clip_error: bool,
}

impl<C: Component> PaintContext<'_, C> {
    pub fn bounds(&self) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: self.width,
            height: self.height,
        }
    }
    pub fn emit(&mut self, command: DrawCommand) {
        let rect = |mut rect: Rect| {
            rect.x += self.origin.x;
            rect.y += self.origin.y;
            rect
        };
        let command = match command {
            DrawCommand::FillRect {
                rect: bounds,
                color,
            } => DrawCommand::FillRect {
                rect: rect(bounds),
                color,
            },
            DrawCommand::StrokeRect {
                rect: bounds,
                color,
                width,
            } => DrawCommand::StrokeRect {
                rect: rect(bounds),
                color,
                width,
            },
            DrawCommand::DrawText {
                mut origin,
                text,
                font,
                color,
            } => {
                origin.x += self.origin.x;
                origin.y += self.origin.y;
                DrawCommand::DrawText {
                    origin,
                    text,
                    font,
                    color,
                }
            }
            DrawCommand::DrawImage { image, destination } => DrawCommand::DrawImage {
                image,
                destination: rect(destination),
            },
            DrawCommand::PushClip { rect: bounds } => {
                self.clip_depth += 1;
                DrawCommand::PushClip { rect: rect(bounds) }
            }
            DrawCommand::PopClip => {
                if self.clip_depth == 0 {
                    self.clip_error = true;
                    return;
                }
                self.clip_depth -= 1;
                DrawCommand::PopClip
            }
        };
        self.display.emit(command);
    }
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.emit(DrawCommand::FillRect { rect, color });
    }
    pub fn stroke_rect(&mut self, rect: Rect, color: Color, width: f32) {
        self.emit(DrawCommand::StrokeRect { rect, color, width });
    }
    /// Draw normalized unkerned text at a local baseline using embedded Geist.
    /// Metrics may be queried first; glyph coverage is prepared after painting.
    pub fn text(
        &mut self,
        mut origin: Point,
        text: impl Into<String>,
        size: f32,
        color: Color,
    ) -> PixuiResult<()> {
        let config = self.font_config(size)?;
        origin.x += self.origin.x;
        origin.y += self.origin.y;
        self.display.text(config, origin, text, color)
    }
    pub fn font_metrics(&self, size: f32) -> PixuiResult<FontMetrics> {
        Ok(self.font_config(size)?.metrics())
    }
    pub fn measure_text(&self, text: &str, size: f32) -> PixuiResult<Size> {
        self.display.measure_text(&self.font_config(size)?, text)
    }
    fn font_config(&self, size: f32) -> PixuiResult<FontConfig> {
        FontConfig::new(FontFace::geist()?, size, self.settings.scale_factor)
    }
    /// Registers the snapshot once in the shared image table and draws locally.
    pub fn image(&mut self, image: &Image, destination: Rect) {
        let image = self.display.image_index(image);
        self.emit(DrawCommand::DrawImage { image, destination });
    }
    /// Schedules a future worker render; does not mutate component state.
    pub fn request_redraw_after(&mut self, delay: std::time::Duration) {
        self.display.request_redraw_after(delay);
    }
    pub fn line_height(&self, size: f32) -> PixuiResult<f32> {
        Ok(self.font_metrics(size)?.line_height)
    }
    /// Restores the clip even when the nested drawing operation returns an error.
    pub fn with_clip(
        &mut self,
        rect: Rect,
        draw: impl FnOnce(&mut Self) -> PixuiResult<()>,
    ) -> PixuiResult<()> {
        self.emit(DrawCommand::PushClip { rect });
        let result = draw(self);
        self.emit(DrawCommand::PopClip);
        result
    }
}
