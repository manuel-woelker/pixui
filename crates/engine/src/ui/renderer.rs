//! Headless layout and display-list generation using the existing part walker.
//! The initial layout is a vertical stack; composites and loops group traversal
//! without introducing layout boxes. Every visible component occupies one row.

use super::{
    display_list::{Color, DisplayList, DrawCommand, FontId},
    geometry::{Point, Rect},
    instance::{HitRegion, LayoutState},
    presentation::{PresentationSettings, Theme},
    text,
    widget::Widget,
};
use crate::{
    application::app::Application,
    expression::context::ExpressionContext,
    live_model::{
        part::LivePart,
        state::LiveState,
        walk::{Visitor, WalkEntry, walk},
    },
};
use pixui_base::PixuiResult;

const PADDING: f32 = 16.0;
const FONT_SIZE: f32 = 16.0;

struct CollectWidgets<'a> {
    settings: &'a PresentationSettings,
    widgets: Vec<Widget>,
}

impl Visitor for CollectWidgets<'_> {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
        if let LivePart::Component(component) = &*entry.part
            && let Some(presentation) = component.presentation
        {
            self.widgets
                .push(presentation(entry.context, self.settings)?);
        }
        Ok(())
    }
}

/// Walks a private template copy, retaining only the instance's physical state.
/// Geometry is measured before painting. Failures return no partial output;
/// component initialization already performed by the walker is not rolled back.
pub fn render(
    template: &LivePart,
    state: &mut LiveState,
    application: &Application,
    settings: &PresentationSettings,
    scroll: f32,
    focus: Option<usize>,
    hover: Option<usize>,
) -> PixuiResult<(DisplayList, LayoutState, f32)> {
    settings.validate()?;
    let mut template = template.clone();
    let mut visitor = CollectWidgets {
        settings,
        widgets: Vec::new(),
    };
    walk(
        &mut template,
        state.root_state_mut(),
        &ExpressionContext::new(application),
        &mut visitor,
    )?;
    let width = (settings.viewport.width - 2.0 * PADDING).max(0.0);
    let mut rows = Vec::new();
    let mut y = PADDING;
    for widget in visitor.widgets {
        let (text, inset) = match &widget {
            Widget::Label { text } => (text, 0.0),
            Widget::Button { text, .. } => (text, 8.0),
            Widget::Checkbox { text, .. } => (text, 32.0),
        };
        let lines = text::wrap(text, FONT_SIZE, (width - inset - 8.0).max(0.0));
        let height = lines.len() as f32 * text::line_height(FONT_SIZE) + 16.0;
        rows.push((
            widget,
            lines,
            inset,
            Rect {
                x: PADDING,
                y,
                width,
                height,
            },
        ));
        y += height + 8.0;
    }
    let scroll = scroll.clamp(0.0, (y + PADDING - settings.viewport.height).max(0.0));
    let (background, foreground, control, accent) = match settings.theme {
        Theme::Light => (
            Color(250, 250, 250),
            Color(25, 25, 25),
            Color(225, 230, 238),
            Color(35, 95, 200),
        ),
        Theme::Dark => (
            Color(25, 28, 34),
            Color(235, 235, 240),
            Color(55, 60, 70),
            Color(130, 180, 255),
        ),
    };
    let viewport = Rect {
        x: 0.0,
        y: 0.0,
        width: settings.viewport.width,
        height: settings.viewport.height,
    };
    let mut display = DisplayList {
        commands: vec![
            DrawCommand::FillRect {
                rect: viewport,
                color: background,
            },
            DrawCommand::PushClip { rect: viewport },
        ],
    };
    let mut layout = LayoutState {
        content_height: y + PADDING,
        ..Default::default()
    };
    for (widget, lines, inset, mut rect) in rows {
        rect.y -= scroll;
        layout.component_bounds.push(rect);
        let interactive = !matches!(widget, Widget::Label { .. });
        let index = layout.hit_regions.len();
        if interactive {
            display.commands.push(DrawCommand::FillRect {
                rect,
                color: control,
            });
            if focus == Some(index) || hover == Some(index) {
                display.commands.push(DrawCommand::StrokeRect {
                    rect,
                    color: accent,
                    width: 2.0,
                });
            }
        }
        if let Widget::Checkbox { checked, .. } = &widget {
            let box_rect = Rect {
                x: rect.x + 6.0,
                y: rect.y + 8.0,
                width: 18.0,
                height: 18.0,
            };
            display.commands.push(DrawCommand::StrokeRect {
                rect: box_rect,
                color: foreground,
                width: 2.0,
            });
            if *checked {
                display.commands.push(DrawCommand::FillRect {
                    rect: Rect {
                        x: box_rect.x + 4.0,
                        y: box_rect.y + 4.0,
                        width: 10.0,
                        height: 10.0,
                    },
                    color: accent,
                });
            }
        }
        display.commands.push(DrawCommand::PushClip { rect });
        for (line, content) in lines.into_iter().enumerate() {
            display.commands.push(DrawCommand::DrawText {
                origin: Point {
                    x: rect.x + inset,
                    y: rect.y + 8.0 + line as f32 * text::line_height(FONT_SIZE),
                },
                text: content,
                font: FontId::Builtin,
                size: FONT_SIZE,
                color: foreground,
            });
        }
        display.commands.push(DrawCommand::PopClip);
        match widget {
            Widget::Label { .. } => {}
            Widget::Button { activate, .. } | Widget::Checkbox { activate, .. } => {
                layout.hit_regions.push(HitRegion {
                    bounds: rect.intersect(viewport),
                    activate,
                });
            }
        }
    }
    display.commands.push(DrawCommand::PopClip);
    display.validate()?;
    Ok((display, layout, scroll))
}
