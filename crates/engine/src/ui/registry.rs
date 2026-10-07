//! Application-owned definitions, instances, revision-aware input, and rendering.

use super::{
    definition::{UiDefinition, UiDefinitionId},
    display_list::{RenderOutput, RenderRevision},
    input::{UiCommand, UiInput},
    input_handler::{InputIntent, interpret},
    instance::{LayoutState, UiInstance, UiInstanceId},
    mailbox::{OutputReceiver, mailbox},
    presentation::PresentationSettings,
    renderer,
};
use crate::{
    application::{action::ActionCall, app::Application},
    live_model::state::LiveState,
};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> PixuiResult<u64> {
    NEXT_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| pixui_error!("UI identity exhausted"))
}

#[derive(Default)]
pub struct UiRegistry {
    definitions: HashMap<UiDefinitionId, UiDefinition>,
    names: HashMap<String, UiDefinitionId>,
    instances: HashMap<UiInstanceId, UiInstance>,
}

impl UiRegistry {
    pub fn register(&mut self, definition: UiDefinition) -> PixuiResult<UiDefinitionId> {
        if definition.name.is_empty() || self.names.contains_key(&definition.name) {
            return Err(pixui_error!("empty or duplicate UI definition name"));
        }
        let id = UiDefinitionId(next_id()?);
        self.names.insert(definition.name.clone(), id);
        self.definitions.insert(id, definition);
        Ok(id)
    }

    pub fn definition(&self, id: UiDefinitionId) -> PixuiResult<&UiDefinition> {
        self.definitions
            .get(&id)
            .ok_or_else(|| pixui_error!("unknown UI definition"))
    }

    pub fn definition_id(&self, name: &str) -> PixuiResult<UiDefinitionId> {
        self.names
            .get(name)
            .copied()
            .ok_or_else(|| pixui_error!("unknown UI definition `{name}`"))
    }

    pub fn create(
        &mut self,
        definition: UiDefinitionId,
        settings: PresentationSettings,
    ) -> PixuiResult<(UiInstanceId, OutputReceiver)> {
        settings.validate()?;
        if !self.definitions.contains_key(&definition) {
            return Err(pixui_error!("unknown UI definition"));
        }
        let id = UiInstanceId(next_id()?);
        let (outputs, receiver) = mailbox();
        self.instances.insert(
            id,
            UiInstance {
                definition,
                state: LiveState::new(),
                settings,
                layout: LayoutState::default(),
                revision: RenderRevision::default(),
                compatible_revision: RenderRevision::default(),
                redraw_only: false,
                dirty: true,
                window_properties_dirty: true,
                window_properties: None,
                window_properties_error: None,
                geometry_stale: true,
                outputs,
                error: None,
                animation_request: None,
                visible: true,
                overlay: Default::default(),
                last_render: None,
                diagnostics_dirty: false,
            },
        );
        Ok((id, receiver))
    }

    pub fn instance(&self, id: UiInstanceId) -> PixuiResult<&UiInstance> {
        self.instances
            .get(&id)
            .ok_or_else(|| pixui_error!("unknown UI instance"))
    }

    pub fn close(&mut self, id: UiInstanceId) -> PixuiResult<()> {
        self.instances
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| pixui_error!("unknown UI instance"))
    }

    /// Content changes invalidate positional focus/hover. Scroll is retained and
    /// clamped after layout. Stable loop reconciliation can relax this later.
    pub fn invalidate_all(&mut self) {
        for definition in self.definitions.values_mut() {
            definition.state.focus = None;
            definition.state.hover = None;
        }
        for instance in self.instances.values_mut() {
            instance.dirty = true;
            instance.window_properties_dirty = true;
            instance.redraw_only = false;
            instance.geometry_stale = true;
        }
    }

    /// Resolves an input binding without borrowing the registry during dispatch.
    /// Discrete input requires a compatible revision; visual redraws preserve older
    /// presented revisions while geometry and targets match. Stale pointer motion is
    /// superseded and discarded. Stale activations return errors, never retarget.
    pub fn command(
        &mut self,
        command: UiCommand,
        application: &Application,
    ) -> PixuiResult<Option<ActionCall>> {
        if let UiCommand::Close { instance } = command {
            self.close(instance)?;
            return Ok(None);
        }
        if let UiCommand::Input {
            instance,
            revision,
            input,
        } = command
        {
            return self.input(instance, revision, input, application);
        }
        let instance = self
            .instances
            .get_mut(&command.instance())
            .ok_or_else(|| pixui_error!("unknown UI instance"))?;
        if !matches!(
            command,
            UiCommand::Redraw { .. }
                | UiCommand::AnimationFrame { .. }
                | UiCommand::Visibility { .. }
                | UiCommand::FramePresented { .. }
        ) {
            instance.redraw_only = false;
        }
        match command {
            UiCommand::FramePresented {
                paint_revision,
                timings,
                timestamp,
                ..
            } => {
                if paint_revision.0 == 0 || paint_revision > instance.revision {
                    return Err(pixui_error!("invalid presented paint revision"));
                }
                instance
                    .overlay
                    .presented(paint_revision, timings, timestamp);
            }
            UiCommand::Visibility { visible, .. } => {
                if instance.visible != visible {
                    instance.visible = visible;
                    // Preserve accumulated dirty state while hidden. Showing also
                    // refreshes time-dependent drawing when no actions occurred.
                    if visible {
                        if !instance.dirty {
                            instance.redraw_only = true;
                        }
                        instance.dirty = true;
                        instance.window_properties_dirty = true;
                    }
                }
            }
            UiCommand::AnimationFrame { request, .. } => {
                if instance
                    .animation_request
                    .is_some_and(|previous| request <= previous)
                {
                    return Err(pixui_error!("animation request ID must increase"));
                }
                instance.animation_request = Some(request);
                if !instance.dirty {
                    instance.redraw_only = true;
                }
                instance.dirty = true;
            }
            UiCommand::Redraw { .. } => {
                if !instance.dirty {
                    instance.redraw_only = true;
                }
                instance.dirty = true;
                instance.window_properties_dirty = true;
            }
            UiCommand::Present { settings, .. } => {
                settings.validate()?;
                application
                    .translations()
                    .validate_language(settings.language)?;
                instance.settings = settings;
                instance.dirty = true;
                instance.window_properties_dirty = true;
                instance.geometry_stale = true;
            }
            UiCommand::Input { .. } => unreachable!(),
            UiCommand::Close { .. } => unreachable!(),
        }
        Ok(None)
    }

    fn input(
        &mut self,
        id: UiInstanceId,
        revision: RenderRevision,
        input: UiInput,
        application: &Application,
    ) -> PixuiResult<Option<ActionCall>> {
        let intent = interpret(input)?;
        let instance = self
            .instances
            .get_mut(&id)
            .ok_or_else(|| pixui_error!("unknown UI instance"))?;
        // Diagnostics and unhandled raw events do not depend on displayed geometry.
        if matches!(intent, InputIntent::ToggleDiagnostics) {
            instance.overlay.toggle();
            instance.diagnostics_dirty = true;
            return Ok(None);
        }
        if matches!(intent, InputIntent::Ignore) {
            return Ok(None);
        }
        if !matches!(intent, InputIntent::ClearHover)
            && (revision < instance.compatible_revision
                || revision > instance.revision
                || revision.0 == 0
                || instance.geometry_stale)
        {
            if matches!(intent, InputIntent::Hover(_)) {
                return Ok(None);
            }
            return Err(pixui_error!("stale UI input revision"));
        }
        let definition = instance.definition;
        let state = &mut self
            .definitions
            .get_mut(&definition)
            .expect("registered definition")
            .state;
        let before = state.clone();
        let mut geometry_changed = false;
        let mut action = None;
        match intent {
            InputIntent::ClearHover => state.hover = None,
            InputIntent::Hover(point) => {
                let viewport = super::geometry::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: instance.settings.viewport.width,
                    height: instance.settings.viewport.height,
                };
                state.hover = instance
                    .layout
                    .component_bounds
                    .iter()
                    .rposition(|bounds| bounds.intersect(viewport).contains(point));
            }
            InputIntent::Activate(point) => {
                let region = instance
                    .layout
                    .hit_regions
                    .iter()
                    .rev()
                    .find(|region| region.bounds.contains(point));
                state.focus = region.map(|region| region.component_index);
                action = region
                    .map(|region| (region.activate)(application))
                    .transpose()?;
            }
            InputIntent::ActivateFocused => {
                let region = instance
                    .layout
                    .hit_regions
                    .iter()
                    .find(|region| Some(region.component_index) == state.focus);
                action = region
                    .map(|region| (region.activate)(application))
                    .transpose()?;
            }
            InputIntent::FocusNext { backwards } => {
                let regions = &instance.layout.hit_regions;
                let current = regions
                    .iter()
                    .position(|region| Some(region.component_index) == state.focus);
                let next = if regions.is_empty() {
                    None
                } else {
                    Some(match (current, backwards) {
                        (Some(0) | None, true) => regions.len() - 1,
                        (Some(index), true) => index - 1,
                        (Some(index), false) => (index + 1) % regions.len(),
                        (None, false) => 0,
                    })
                };
                state.focus = next.map(|index| regions[index].component_index);
            }
            InputIntent::Scroll(delta) => {
                let maximum =
                    (instance.layout.content_height - instance.settings.viewport.height).max(0.0);
                state.scroll = (state.scroll + delta).clamp(0.0, maximum);
                geometry_changed = state.scroll != before.scroll;
            }
            InputIntent::ToggleDiagnostics | InputIntent::Ignore => unreachable!(),
        }
        if *state != before {
            for peer in self
                .instances
                .values_mut()
                .filter(|peer| peer.definition == definition)
            {
                if geometry_changed {
                    peer.geometry_stale = true;
                    peer.redraw_only = false;
                } else if !peer.dirty {
                    peer.redraw_only = true;
                }
                peer.dirty = true;
                peer.window_properties_dirty = true;
            }
        }
        Ok(action)
    }

    /// Earliest diagnostic refresh across visible windows. The worker can sleep
    /// until this deadline without requiring host-side overlay timers.
    pub(crate) fn next_refresh(&self) -> Option<Instant> {
        self.instances
            .values()
            .filter(|instance| instance.visible && instance.outputs.connected())
            .filter_map(|instance| instance.overlay.refresh)
            .min()
    }

    /// Publishes only complete successful renders, preserving the last good
    /// revision and geometry on failure. Errors remain inspectable on the worker.
    /// Dropped consumers release their instance on the next rendering pass.
    pub fn render_dirty(&mut self, application: &Application) {
        self.instances
            .retain(|_, instance| instance.outputs.connected());
        let now = Instant::now();
        for (id, instance) in &mut self.instances {
            if instance.window_properties_dirty {
                instance.window_properties_dirty = false;
                let definition = &self.definitions[&instance.definition];
                instance.window_properties_error =
                    update_window_properties(instance, definition, application)
                        .err()
                        .map(|error| format!("{error:?}"));
            }
            if !instance.visible {
                continue;
            }
            if !instance.dirty {
                let refresh_due = instance.diagnostics_dirty
                    || instance
                        .overlay
                        .refresh
                        .is_some_and(|deadline| deadline <= now);
                if refresh_due && let Some(output) = &instance.last_render {
                    let mut output = output.clone();
                    let Some(revision) = instance.revision.0.checked_add(1) else {
                        instance.error = Some("render revision exhausted".into());
                        instance.overlay.refresh = None;
                        instance.diagnostics_dirty = false;
                        continue;
                    };
                    output.revision = RenderRevision(revision);
                    publish(instance, output, now);
                }
                continue;
            }
            instance.dirty = false;
            let Some(revision) = instance.revision.0.checked_add(1) else {
                instance.error = Some("render revision exhausted".into());
                continue;
            };
            let definition = &self.definitions[&instance.definition];
            let result = renderer::render_measured(
                &definition.template,
                &mut instance.state,
                application,
                &instance.settings,
                definition.state.scroll,
                definition.state.focus,
                definition.state.hover,
            );
            match result {
                Ok(renderer::RenderedUi {
                    display_list,
                    mut layout,
                    scroll: _,
                    redraw_after,
                    timings,
                    animating,
                }) => {
                    let mut output = RenderOutput {
                        instance_id: *id,
                        revision: RenderRevision(revision),
                        paint_revision: RenderRevision(revision),
                        display_list,
                        redraw_after,
                        timings,
                        animating,
                        animation_request: instance.animation_request,
                    };
                    let cached = output.clone();
                    if let Err(error) = decorate(instance, &mut output, now) {
                        instance.error = Some(format!("{error:?}"));
                        continue;
                    }
                    // Visual-only redraws retain their existing action targets.
                    // Older presented revisions remain usable while geometry and
                    // content identity are unchanged, avoiding animated click races.
                    let same_geometry = layout.component_bounds == instance.layout.component_bounds
                        && layout
                            .hit_regions
                            .iter()
                            .map(|region| region.bounds)
                            .eq(instance
                                .layout
                                .hit_regions
                                .iter()
                                .map(|region| region.bounds));
                    if instance.redraw_only && !instance.geometry_stale && same_geometry {
                        layout.hit_regions = std::mem::take(&mut instance.layout.hit_regions);
                    } else {
                        instance.compatible_revision = RenderRevision(revision);
                    }
                    instance.redraw_only = false;
                    instance.layout = layout;
                    instance.geometry_stale = false;
                    instance.error = None;
                    instance.last_render = Some(cached);
                    instance.revision = output.revision;
                    instance.outputs.publish(output);
                }
                Err(error) => instance.error = Some(format!("{error:?}")),
            }
        }
    }
}

/// Diagnostic drawing is applied only to a complete output. Failure preserves
/// the previously published frame; cached application commands stay unmodified.
fn publish(instance: &mut UiInstance, mut output: RenderOutput, now: Instant) {
    if let Err(error) = decorate(instance, &mut output, now) {
        instance.error = Some(format!("{error:?}"));
        return;
    }
    instance.revision = output.revision;
    instance.outputs.publish(output);
}

fn decorate(instance: &mut UiInstance, output: &mut RenderOutput, now: Instant) -> PixuiResult<()> {
    instance.diagnostics_dirty = false;
    instance.overlay.refresh = None;
    if instance.overlay.visible {
        output.display_list = instance.overlay.append(
            std::mem::take(&mut output.display_list),
            output.timings,
            instance.settings.scale_factor,
            instance.settings.viewport,
            now,
        )?;
    }
    Ok(())
}

fn update_window_properties(
    instance: &mut UiInstance,
    definition: &UiDefinition,
    application: &Application,
) -> PixuiResult<()> {
    use super::window_properties::{ResolvedWindowProperties, WindowCommand};
    let context = crate::expression::context::ExpressionContext::new(application)
        .with_language(instance.settings.language);
    let properties = if let Some(binding) = &definition.window_expressions {
        let values = binding
            .expressions
            .iter()
            .map(|expression| crate::expression::evaluator::evaluate(&context, expression))
            .collect::<PixuiResult<Vec<_>>>()?;
        (binding.resolve)(&context, &instance.settings, &values)?
    } else if let Some(resolve) = definition.window_properties {
        resolve(&context, &instance.settings)?
    } else {
        return Ok(());
    };
    let resolved = ResolvedWindowProperties {
        title: properties.title,
        icon: properties
            .icon
            .as_ref()
            .map(|path| application.load_image(path))
            .transpose()?,
    };
    let previous = instance.window_properties.as_ref();
    if previous.is_none_or(|previous| previous.title != resolved.title) {
        instance
            .outputs
            .window_commands
            .publish(WindowCommand::SetTitle(resolved.title.clone()));
    }
    if previous.is_none_or(|previous| previous.icon != resolved.icon) {
        instance
            .outputs
            .window_commands
            .publish(WindowCommand::SetIcon(resolved.icon.clone()));
    }
    instance.window_properties = Some(resolved);
    Ok(())
}
