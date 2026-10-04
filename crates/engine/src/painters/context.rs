//! Typed, borrowed input and a local drawing-command sink for one component.

use crate::{
    live_model::component::Component,
    ui::{
        display_list::{Color, DisplayList, DrawCommand, FontId},
        geometry::{Point, Rect},
        presentation::PresentationSettings,
        text,
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
    pub(crate) display: &'a mut DisplayList,
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
        self.display.commands.push(command);
    }
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.emit(DrawCommand::FillRect { rect, color });
    }
    pub fn stroke_rect(&mut self, rect: Rect, color: Color, width: f32) {
        self.emit(DrawCommand::StrokeRect { rect, color, width });
    }
    pub fn text(&mut self, origin: Point, text: impl Into<String>, size: f32, color: Color) {
        self.emit(DrawCommand::DrawText {
            origin,
            text: text.into(),
            font: FontId::Builtin,
            size,
            color,
        });
    }
    pub fn line_height(&self, size: f32) -> f32 {
        text::line_height(size)
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
