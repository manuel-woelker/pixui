//! Fixed-height component painting. There is no measurement or general layout API.

use super::{
    activation::ActionBinding,
    display_list::{DisplayList, DrawCommand},
    geometry::{Point, Rect},
    instance::{HitRegion, LayoutState},
    presentation::PresentationSettings,
};
use crate::{
    application::app::Application,
    expression::context::ExpressionContext,
    live_model::{
        part::LivePart,
        state::{LiveState, PartState},
        walk::{Visitor, WalkEntry, walk},
    },
    painters::{palette::Palette, registry::PaintInput},
};
use pixui_base::{PixuiResult, pixui_error};

pub const COMPONENT_HEIGHT: f32 = 36.0;
const PADDING: f32 = 16.0;
const SPACING: f32 = 8.0;

struct PaintedComponent {
    display: DisplayList,
    activate: Option<ActionBinding>,
}
struct PaintVisitor<'a> {
    application: &'a Application,
    settings: &'a PresentationSettings,
    width: f32,
    focus: Option<usize>,
    hover: Option<usize>,
    interactive_count: usize,
    components: Vec<PaintedComponent>,
}

impl Visitor for PaintVisitor<'_> {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
        let LivePart::Component(part) = &*entry.part else {
            return Ok(());
        };
        let Some(binding) = &part.binding else {
            return Ok(());
        };
        let PartState::Component(state) = entry.state else {
            return Err(pixui_error!("component requires initialized state"));
        };
        let props = binding.prepare(entry.context, self.settings, &mut state.state)?;
        let activate = part
            .activation
            .map(|factory| factory(entry.context, self.settings))
            .transpose()?;
        let interactive = activate.is_some();
        let index = self.interactive_count;
        let display = self.application.painters().paint(
            binding.address(),
            self.application.components(),
            PaintInput {
                props: props.as_ref(),
                state: &state.state,
                settings: self.settings,
                width: self.width,
                height: COMPONENT_HEIGHT,
                focused: interactive && self.focus == Some(index),
                hovered: interactive && self.hover == Some(index),
            },
        )?;
        if interactive {
            self.interactive_count += 1;
        }
        self.components.push(PaintedComponent { display, activate });
        Ok(())
    }
}

/// Prepares props/updates and paints exactly once per physical component. Only
/// owned local commands and bindings survive traversal. Translation and clipping
/// are applied after clamping scrolling against the fixed-height content.
/// Rendering errors publish no partial geometry or commands; state updates are
/// not rolled back. Legacy components without typed registration remain inert.
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
    application
        .components()
        .validate(template, application.painters())?;
    let mut template = template.clone();
    let width = (settings.viewport.width - 2.0 * PADDING).max(0.0);
    let mut visitor = PaintVisitor {
        application,
        settings,
        width,
        focus,
        hover,
        interactive_count: 0,
        components: Vec::new(),
    };
    walk(
        &mut template,
        state.root_state_mut(),
        &ExpressionContext::new(application),
        &mut visitor,
    )?;
    let count = visitor.components.len();
    let content_height =
        2.0 * PADDING + count as f32 * COMPONENT_HEIGHT + count.saturating_sub(1) as f32 * SPACING;
    let scroll = scroll.clamp(0.0, (content_height - settings.viewport.height).max(0.0));
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
                color: Palette::for_theme(settings.theme).background,
            },
            DrawCommand::PushClip { rect: viewport },
        ],
    };
    let mut layout = LayoutState {
        content_height,
        ..Default::default()
    };
    for (index, component) in visitor.components.into_iter().enumerate() {
        let bounds = Rect {
            x: PADDING,
            y: PADDING + index as f32 * (COMPONENT_HEIGHT + SPACING) - scroll,
            width,
            height: COMPONENT_HEIGHT,
        };
        layout.component_bounds.push(bounds);
        display
            .commands
            .push(DrawCommand::PushClip { rect: bounds });
        for command in component.display.commands {
            display.commands.push(translate(
                command,
                Point {
                    x: bounds.x,
                    y: bounds.y,
                },
            ));
        }
        display.commands.push(DrawCommand::PopClip);
        if let Some(activate) = component.activate {
            layout.hit_regions.push(HitRegion {
                bounds: bounds.intersect(viewport),
                activate,
            });
        }
    }
    display.commands.push(DrawCommand::PopClip);
    display.validate()?;
    Ok((display, layout, scroll))
}

fn translate(command: DrawCommand, offset: Point) -> DrawCommand {
    let rect = |mut rect: Rect| {
        rect.x += offset.x;
        rect.y += offset.y;
        rect
    };
    match command {
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
        DrawCommand::PushClip { rect: bounds } => DrawCommand::PushClip { rect: rect(bounds) },
        DrawCommand::DrawText {
            mut origin,
            text,
            font,
            size,
            color,
        } => {
            origin.x += offset.x;
            origin.y += offset.y;
            DrawCommand::DrawText {
                origin,
                text,
                font,
                size,
                color,
            }
        }
        DrawCommand::PopClip => DrawCommand::PopClip,
    }
}
