//! Prepare once, then paint into one shared builder at final hierarchical layout positions.

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
    painters::{
        palette::Palette,
        registry::{MeasureInput, PaintInput},
    },
};
use pixui_base::{PixuiResult, pixui_error};
use std::{any::Any, time::Duration};

struct PreparedComponent {
    address: ComponentAddress,
    props: Box<dyn Any + Send>,
    activate: Option<ActionBinding>,
    activation_factory: Option<usize>,
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
        let values = part
            .expressions
            .iter()
            .map(|expression| crate::expression::evaluator::evaluate(entry.context, expression))
            .collect::<PixuiResult<Vec<_>>>()?;
        let props = binding.prepare(entry.context, self.settings, &mut state.state, &values)?;
        let activate = part
            .activation
            .map(|factory| factory(entry.context, self.settings))
            .transpose()?;
        self.components.push(PreparedComponent {
            address: binding.address(),
            props,
            activate,
            activation_factory: part.activation.map(|factory| factory as usize),
        });
        Ok(())
    }
}

/// Preparing finishes before painting. Resolvers and updates run once per node.
/// Commands go directly to one shared builder; errors discard it, but do not
/// roll back updates. Legacy nodes remain inert. The fourth result is the
/// earliest painter-requested redraw delay for native or headless scheduling.
/// Use `render_measured` to inspect continuous animation requests and timings.
pub fn render(
    template: &LivePart,
    state: &mut LiveState,
    application: &Application,
    settings: &PresentationSettings,
    scroll: f32,
    focus: Option<usize>,
    hover: Option<usize>,
) -> PixuiResult<(DisplayList, LayoutState, f32, Option<Duration>)> {
    let rendered = render_measured(template, state, application, settings, scroll, focus, hover)?;
    Ok((
        rendered.display_list,
        rendered.layout,
        rendered.scroll,
        rendered.redraw_after,
    ))
}

/// Complete worker render, including native scheduling hints and CPU diagnostics.
pub struct RenderedUi {
    pub display_list: DisplayList,
    pub layout: LayoutState,
    pub scroll: f32,
    pub redraw_after: Option<Duration>,
    pub animating: bool,
    pub(crate) hover: Option<usize>,
    pub timings: super::performance::WorkerTimings,
}

/// Like `render`, with CPU durations for preparation, tree construction, layout,
/// painting, and text finalization.
pub fn render_measured(
    template: &LivePart,
    state: &mut LiveState,
    application: &Application,
    settings: &PresentationSettings,
    scroll: f32,
    focus: Option<usize>,
    hover: Option<usize>,
) -> PixuiResult<RenderedUi> {
    render_with_pointer(
        template,
        state,
        application,
        settings,
        scroll,
        focus,
        hover,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_with_pointer(
    template: &LivePart,
    state: &mut LiveState,
    application: &Application,
    settings: &PresentationSettings,
    scroll: f32,
    focus: Option<usize>,
    hover: Option<usize>,
    pointer: Option<Point>,
) -> PixuiResult<RenderedUi> {
    let started = std::time::Instant::now();
    settings.validate()?;
    application
        .translations()
        .validate_language(settings.language)?;
    let timestamp_us = settings
        .timestamp_us
        .unwrap_or_else(|| application.render_clock.timestamp_us());
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
        &ExpressionContext::new(application).with_language(settings.language),
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
            PartState::Composite(state) | PartState::Container(state) => {
                pending.extend(state.parts.iter().rev())
            }
            PartState::ForLoop(state) => pending.extend(state.items.iter().rev()),
            PartState::Match(state) if state.selected.is_some() => pending.push(&state.part),
            _ => {}
        }
    }
    if states.len() != visitor.components.len() {
        return Err(pixui_error!("prepared component state count mismatch"));
    }
    crate::ui::text::font::FontFace::geist()?;
    let preparation = started.elapsed();
    let geometry = crate::layout::adapter::compute(
        &template,
        state.root_state(),
        settings.viewport,
        scroll,
        |index, constraints| {
            let component = &visitor.components[index];
            application.painters().measure(
                component.address,
                application.components(),
                MeasureInput {
                    props: component.props.as_ref(),
                    state: states[index],
                    settings,
                    constraints,
                },
            )
        },
    )?;
    let hover = pointer.map_or(hover, |point| {
        geometry
            .leaves
            .iter()
            .rposition(|leaf| leaf.clip.contains(point))
    });
    let scroll = geometry.scroll;
    let viewport = Rect {
        x: 0.0,
        y: 0.0,
        width: settings.viewport.width,
        height: settings.viewport.height,
    };
    let started = std::time::Instant::now();
    let mut display = DisplayListBuilder::default();
    display.emit(DrawCommand::FillRect {
        rect: viewport,
        color: Palette::for_theme(settings.theme).background,
    });
    display.emit(DrawCommand::PushClip { rect: viewport });
    let mut layout = LayoutState {
        content_height: geometry.content_height,
        container_bounds: geometry.containers,
        scroll_offset: scroll,
        ..Default::default()
    };
    for (index, (component, state)) in visitor.components.into_iter().zip(states).enumerate() {
        if state.component_address() != Some(component.address) {
            return Err(pixui_error!("prepared component identity mismatch"));
        }
        let geometry = &geometry.leaves[index];
        let bounds = geometry.border;
        let content = geometry.content;
        layout.component_bounds.push(bounds);
        layout.component_addresses.push(component.address);
        layout
            .activation_factories
            .push(component.activation_factory);
        layout.content_bounds.push(content);
        layout.component_clips.push(geometry.clip);
        display.emit(DrawCommand::PushClip {
            rect: geometry.clip.intersect(content),
        });
        application.painters().paint(
            component.address,
            application.components(),
            PaintInput {
                props: component.props.as_ref(),
                timestamp_us,
                state,
                settings,
                width: content.width,
                height: content.height,
                focused: focus == Some(index),
                hovered: hover == Some(index),
                origin: Point {
                    x: content.x,
                    y: content.y,
                },
            },
            &mut display,
        )?;
        display.emit(DrawCommand::PopClip);
        if let Some(activate) = component.activate {
            let target_index = layout.focus_targets.len();
            layout.focus_targets.push(super::instance::FocusTarget {
                component_index: index,
                activate,
            });
            if geometry.clip.width > 0.0 && geometry.clip.height > 0.0 {
                layout.hit_regions.push(HitRegion {
                    bounds: geometry.clip,
                    component_index: index,
                    target_index,
                });
            }
        }
    }
    display.emit(DrawCommand::PopClip);
    let painting = started.elapsed();
    let started = std::time::Instant::now();
    let animating = display.animating();
    let (display, redraw_after) = display.finish_with_text(
        &mut *application
            .text_service
            .try_borrow_mut()
            .map_err(|_| pixui_error!("text finalization is already active"))?,
    )?;
    Ok(RenderedUi {
        display_list: display,
        layout,
        scroll,
        redraw_after,
        animating,
        hover,
        timings: super::performance::WorkerTimings {
            preparation,
            tree_construction: geometry.construction,
            layout: geometry.solving,
            measurements: geometry.measurements,
            painting,
            text: started.elapsed(),
        },
    })
}
