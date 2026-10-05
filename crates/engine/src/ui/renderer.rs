//! Prepare once, then paint into one shared builder at final fixed-row positions.

use super::{
    activation::ActionBinding,
    display_list::{DisplayList, DrawCommand},
    display_list_builder::DisplayListBuilder,
    geometry::{Point, Rect},
    instance::{HitRegion, LayoutState},
    presentation::PresentationSettings,
};
use crate::{
    application::app::Application,
    component_registry::component_id::ComponentAddress,
    expression::context::ExpressionContext,
    live_model::{
        part::LivePart,
        state::{GenericComponentState, LiveState, PartState},
        walk::{Visitor, WalkEntry, walk},
    },
    painters::{palette::Palette, registry::PaintInput},
};
use pixui_base::{PixuiResult, pixui_error};
use std::{any::Any, time::Duration};

pub const COMPONENT_HEIGHT: f32 = 36.0;
const PADDING: f32 = 16.0;
const SPACING: f32 = 8.0;

struct PreparedComponent {
    address: ComponentAddress,
    props: Box<dyn Any + Send>,
    activate: Option<ActionBinding>,
}
struct PrepareVisitor<'a> {
    settings: &'a PresentationSettings,
    components: Vec<PreparedComponent>,
}
impl Visitor for PrepareVisitor<'_> {
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
        self.components.push(PreparedComponent {
            address: binding.address(),
            props,
            activate,
        });
        Ok(())
    }
}

/// Preparing finishes before painting. Resolvers and updates run once per node.
/// Commands go directly to one shared builder; errors discard it, but do not
/// roll back updates. Legacy nodes remain inert. The fourth result is the
/// earliest painter-requested redraw delay for native or headless scheduling.
pub fn render(
    template: &LivePart,
    state: &mut LiveState,
    application: &Application,
    settings: &PresentationSettings,
    scroll: f32,
    focus: Option<usize>,
    hover: Option<usize>,
) -> PixuiResult<(DisplayList, LayoutState, f32, Option<Duration>)> {
    settings.validate()?;
    application
        .components()
        .validate(template, application.painters())?;
    let mut template = template.clone();
    let mut visitor = PrepareVisitor {
        settings,
        components: Vec::new(),
    };
    walk(
        &mut template,
        state.root_state_mut(),
        &ExpressionContext::new(application),
        &mut visitor,
    )?;
    // Reborrow physical state in the same depth-first order without evaluating
    // expressions again, cloning state, or keeping borrowed state through updates.
    let mut states: Vec<&GenericComponentState> = Vec::new();
    let mut pending = vec![state.root_state()];
    while let Some(node) = pending.pop() {
        match node {
            PartState::Component(state) if state.state.component_address().is_some() => {
                states.push(&state.state)
            }
            PartState::Composite(state) => pending.extend(state.parts.iter().rev()),
            PartState::ForLoop(state) => pending.extend(state.items.iter().rev()),
            _ => {}
        }
    }
    if states.len() != visitor.components.len() {
        return Err(pixui_error!("prepared component state count mismatch"));
    }
    let count = visitor.components.len();
    let width = (settings.viewport.width - 2.0 * PADDING).max(0.0);
    let content_height =
        2.0 * PADDING + count as f32 * COMPONENT_HEIGHT + count.saturating_sub(1) as f32 * SPACING;
    let scroll = scroll.clamp(0.0, (content_height - settings.viewport.height).max(0.0));
    let viewport = Rect {
        x: 0.0,
        y: 0.0,
        width: settings.viewport.width,
        height: settings.viewport.height,
    };
    let mut display = DisplayListBuilder::default();
    display.emit(DrawCommand::FillRect {
        rect: viewport,
        color: Palette::for_theme(settings.theme).background,
    });
    display.emit(DrawCommand::PushClip { rect: viewport });
    let mut layout = LayoutState {
        content_height,
        ..Default::default()
    };
    for (index, (component, state)) in visitor.components.into_iter().zip(states).enumerate() {
        if state.component_address() != Some(component.address) {
            return Err(pixui_error!("prepared component identity mismatch"));
        }
        let bounds = Rect {
            x: PADDING,
            y: PADDING + index as f32 * (COMPONENT_HEIGHT + SPACING) - scroll,
            width,
            height: COMPONENT_HEIGHT,
        };
        layout.component_bounds.push(bounds);
        display.emit(DrawCommand::PushClip { rect: bounds });
        let interactive = component.activate.is_some();
        let interaction_index = layout.hit_regions.len();
        application.painters().paint(
            component.address,
            application.components(),
            PaintInput {
                props: component.props.as_ref(),
                state,
                settings,
                width,
                height: COMPONENT_HEIGHT,
                focused: interactive && focus == Some(interaction_index),
                hovered: interactive && hover == Some(interaction_index),
                origin: Point {
                    x: bounds.x,
                    y: bounds.y,
                },
            },
            &mut display,
        )?;
        display.emit(DrawCommand::PopClip);
        if let Some(activate) = component.activate {
            layout.hit_regions.push(HitRegion {
                bounds: bounds.intersect(viewport),
                activate,
            });
        }
    }
    display.emit(DrawCommand::PopClip);
    let (display, redraw_after) = display.finish()?;
    Ok((display, layout, scroll, redraw_after))
}
