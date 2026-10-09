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
    hover_sources: HashMap<UiDefinitionId, UiInstanceId>,
    focus_sources: HashMap<UiDefinitionId, UiInstanceId>,
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
                pointer_position: None,
                painted_hover: None,
                painted_focus: None,
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
            .ok_or_else(|| pixui_error!("unknown UI instance"))?;
        self.refresh_focus_sources();
        self.refresh_hover();
        Ok(())
    }

    /// Content invalidation retains logical identities. Successful source
    /// preparation reconciles eligibility; input still rejects stale geometry.
    pub fn invalidate_all(&mut self) {
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
        if let UiCommand::Focus { instance, target } = command {
            return self.request_focus(instance, target).map(|()| None);
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
            UiCommand::Input { .. } | UiCommand::Focus { .. } => unreachable!(),
            UiCommand::Close { .. } => unreachable!(),
        }
        self.refresh_hover();
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
            InputIntent::ClearHover => {
                instance.pointer_position = None;
                if self.hover_sources.get(&definition) == Some(&id) {
                    self.hover_sources.remove(&definition);
                    state.hover = None;
                }
            }
            InputIntent::Hover(point) => {
                instance.pointer_position = Some(point);
                self.hover_sources.insert(definition, id);
                state.hover = instance
                    .layout
                    .component_clips
                    .iter()
                    .rposition(|bounds| bounds.contains(point))
                    .map(|index| component_id(instance, index));
            }
            InputIntent::Focus(point) | InputIntent::Activate(point) => {
                instance.pointer_position = Some(point);
                self.hover_sources.insert(definition, id);
                state.hover = instance
                    .layout
                    .component_clips
                    .iter()
                    .rposition(|bounds| bounds.contains(point))
                    .map(|index| component_id(instance, index));
                let focus_region = instance
                    .layout
                    .focus_regions
                    .iter()
                    .rev()
                    .find(|region| region.bounds.contains(point));
                state.focus =
                    focus_region.map(|region| component_id(instance, region.component_index));
                self.focus_sources.insert(definition, id);
                if matches!(intent, InputIntent::Activate(_)) {
                    action = instance
                        .layout
                        .hit_regions
                        .iter()
                        .rev()
                        .find(|region| region.bounds.contains(point))
                        .and_then(|region| {
                            instance.layout.activations[region.component_index].as_ref()
                        })
                        .map(|activate| activate(application))
                        .transpose()?;
                }
            }
            InputIntent::ActivateFocused => {
                self.focus_sources.insert(definition, id);
                if !state.focus.as_ref().is_some_and(|focus| {
                    instance
                        .layout
                        .focus_targets
                        .iter()
                        .any(|target| &target.path == focus.path())
                }) {
                    state.focus = None;
                }
                action = state
                    .focus
                    .as_ref()
                    .filter(|focus| {
                        instance
                            .layout
                            .focus_targets
                            .iter()
                            .any(|target| &target.path == focus.path())
                    })
                    .and_then(|focus| instance.layout.component_indices.get(focus.path()))
                    .and_then(|index| instance.layout.activations[*index].as_ref())
                    .map(|activate| activate(application))
                    .transpose()?;
            }
            InputIntent::FocusNext { backwards } => {
                let targets: Vec<_> = instance
                    .layout
                    .focus_targets
                    .iter()
                    .filter(|target| target.sequential)
                    .collect();
                let current = targets.iter().position(|target| {
                    state
                        .focus
                        .as_ref()
                        .is_some_and(|focus| focus.path() == &target.path)
                });
                let next = if targets.is_empty() {
                    None
                } else {
                    Some(match (current, backwards) {
                        (Some(0) | None, true) => targets.len() - 1,
                        (Some(index), true) => index - 1,
                        (Some(index), false) => (index + 1) % targets.len(),
                        (None, false) => 0,
                    })
                };
                state.focus =
                    next.map(|index| component_id(instance, targets[index].component_index));
                self.focus_sources.insert(definition, id);
                if let Some(index) = next {
                    scroll_to_target(state, instance, targets[index].bounds);
                    geometry_changed = state.scroll != before.scroll;
                }
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
        self.refresh_focus_sources();
        self.refresh_hover();
        let now = Instant::now();
        // Render active pointer sources first so peers paint the newly resolved
        // shared hover in the same pass, rather than publishing a second frame.
        let mut ids: Vec<_> = self.instances.keys().copied().collect();
        ids.sort_by_key(|id| {
            let instance = &self.instances[id];
            (
                self.focus_sources.get(&instance.definition) != Some(id),
                self.hover_sources.get(&instance.definition) != Some(id),
                id.0,
            )
        });
        let mut failed = std::collections::HashSet::new();
        // A source reconciliation can dirty a peer already rendered in this pass.
        // A second sweep publishes consistent shared interaction without waking
        // another worker turn or retrying failed preparation.
        for id in ids.iter().chain(ids.iter()) {
            if failed.contains(id) {
                continue;
            }
            let mut focus_changed = false;
            let instance = self.instances.get_mut(id).expect("retained instance");
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
            let result = renderer::render_with_pointer(
                &definition.template,
                &mut instance.state,
                application,
                &instance.settings,
                definition.state.scroll,
                definition.state.focus.as_ref().map(|id| id.path()),
                definition.state.hover.as_ref().map(|id| id.path()),
                if self.hover_sources.get(&instance.definition) == Some(id) {
                    instance.pointer_position
                } else {
                    None
                },
            );
            match result {
                Ok(renderer::RenderedUi {
                    display_list,
                    mut layout,
                    scroll: _,
                    redraw_after,
                    timings,
                    animating,
                    hover,
                    focus,
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
                        failed.insert(*id);
                        continue;
                    }
                    // Visual-only redraws retain their existing action targets.
                    // Older presented revisions remain usable while geometry and
                    // content identity are unchanged, avoiding animated click races.
                    let same_geometry = layout.compatible_with(&instance.layout);
                    if instance.redraw_only && !instance.geometry_stale && same_geometry {
                        // Explicitly keep the published target snapshot for a
                        // compatible visual redraw. Content invalidation and
                        // presentation changes always install fresh bindings.
                        layout.activations = std::mem::take(&mut instance.layout.activations);
                    } else {
                        instance.compatible_revision = RenderRevision(revision);
                    }
                    instance.redraw_only = false;
                    instance.painted_focus =
                        focus.clone().map(|path| super::focus::ComponentInstanceId {
                            definition: instance.definition,
                            path,
                        });
                    instance.painted_hover =
                        hover.clone().map(|path| super::focus::ComponentInstanceId {
                            definition: instance.definition,
                            path,
                        });
                    if self.hover_sources.get(&instance.definition) == Some(id) {
                        self.definitions
                            .get_mut(&instance.definition)
                            .expect("registered definition")
                            .state
                            .hover = instance.painted_hover.clone();
                    }
                    if self.focus_sources.get(&instance.definition) == Some(id) {
                        let definition = self
                            .definitions
                            .get_mut(&instance.definition)
                            .expect("registered definition");
                        let retained = focus
                            .filter(|path| {
                                match (
                                    instance.layout.component_indices.get(path),
                                    layout.component_indices.get(path),
                                ) {
                                    (Some(old), Some(new)) => {
                                        instance.layout.component_addresses[*old]
                                            == layout.component_addresses[*new]
                                    }
                                    _ => true,
                                }
                            })
                            .map(|path| super::focus::ComponentInstanceId {
                                definition: instance.definition,
                                path,
                            });
                        if definition.state.focus != retained {
                            definition.state.focus = retained;
                            focus_changed = true;
                        }
                    }
                    instance.layout = layout;
                    instance.geometry_stale = false;
                    instance.error = None;
                    instance.last_render = Some(cached);
                    instance.revision = output.revision;
                    instance.outputs.publish(output);
                }
                Err(error) => {
                    instance.error = Some(format!("{error:?}"));
                    failed.insert(*id);
                }
            }
            if self.hover_sources.get(&instance.definition) == Some(id) {
                self.refresh_hover();
            }
            if focus_changed {
                self.refresh_focus_paint();
            }
        }
        self.refresh_hover();
    }

    fn request_focus(
        &mut self,
        id: UiInstanceId,
        target: Option<super::focus::ComponentInstanceId>,
    ) -> PixuiResult<()> {
        let instance = self.instance(id)?;
        let definition = instance.definition;
        if let Some(target) = &target
            && (target.definition() != definition
                || instance.geometry_stale
                || !instance
                    .layout
                    .focus_targets
                    .iter()
                    .any(|candidate| &candidate.path == target.path()))
        {
            return Err(pixui_error!("foreign, stale or ineligible focus target"));
        }
        let state = &mut self
            .definitions
            .get_mut(&definition)
            .expect("registered definition")
            .state;
        let before = state.clone();
        if let Some(target) = &target {
            let instance = &self.instances[&id];
            let bounds = instance
                .layout
                .focus_targets
                .iter()
                .find(|candidate| &candidate.path == target.path())
                .expect("validated target")
                .bounds;
            scroll_to_target(state, instance, bounds);
        }
        state.focus = target;
        self.focus_sources.insert(definition, id);
        if *state != before {
            let geometry = state.scroll != before.scroll;
            self.dirty_definition(definition, geometry);
        }
        Ok(())
    }

    fn dirty_definition(&mut self, definition: UiDefinitionId, geometry: bool) {
        for peer in self
            .instances
            .values_mut()
            .filter(|instance| instance.definition == definition)
        {
            if geometry {
                peer.geometry_stale = true;
                peer.redraw_only = false;
            } else if !peer.dirty {
                peer.redraw_only = true;
            }
            peer.dirty = true;
        }
    }

    fn refresh_focus_sources(&mut self) {
        let definitions: Vec<_> = self
            .focus_sources
            .iter()
            .filter(|(_, source)| !self.instances.contains_key(source))
            .map(|(definition, _)| *definition)
            .collect();
        for definition in definitions {
            if let Some(next) = self
                .instances
                .iter()
                .filter(|(_, instance)| instance.definition == definition)
                .map(|(id, _)| *id)
                .min_by_key(|id| id.0)
            {
                self.focus_sources.insert(definition, next);
                self.dirty_definition(definition, false);
            } else {
                self.focus_sources.remove(&definition);
                self.definitions
                    .get_mut(&definition)
                    .expect("registered definition")
                    .state
                    .focus = None;
            }
        }
    }

    fn refresh_focus_paint(&mut self) {
        for instance in self.instances.values_mut() {
            let focus = self.definitions[&instance.definition]
                .state
                .focus
                .as_ref()
                .filter(|focus| {
                    instance
                        .layout
                        .focus_targets
                        .iter()
                        .any(|target| &target.path == focus.path())
                });
            if instance.painted_focus.as_ref() != focus
                && !instance.geometry_stale
                && instance.error.is_none()
            {
                if !instance.dirty {
                    instance.redraw_only = true;
                }
                instance.dirty = true;
            }
        }
    }

    /// Only the most recently active pointer source can update shared hover.
    /// Peers retain independent geometry without fighting over pointer state.
    fn refresh_hover(&mut self) {
        let sources: Vec<_> = self
            .hover_sources
            .iter()
            .map(|(definition, instance)| (*definition, *instance))
            .collect();
        for (definition, id) in sources {
            let source = self
                .instances
                .get(&id)
                .filter(|instance| instance.visible && instance.outputs.connected());
            if source.is_some_and(|instance| instance.geometry_stale) {
                continue;
            }
            let hover = source.and_then(|instance| {
                instance.pointer_position.and_then(|point| {
                    instance
                        .layout
                        .component_clips
                        .iter()
                        .rposition(|bounds| bounds.contains(point))
                        .map(|index| component_id(instance, index))
                })
            });
            if source.is_none() {
                self.hover_sources.remove(&definition);
            }
            let state = &mut self
                .definitions
                .get_mut(&definition)
                .expect("registered definition")
                .state;
            state.hover = hover.clone();
            {
                for peer in self
                    .instances
                    .values_mut()
                    .filter(|peer| peer.definition == definition)
                {
                    if peer.painted_hover != hover {
                        if !peer.dirty {
                            peer.redraw_only = true;
                        }
                        peer.dirty = true;
                    }
                }
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

fn component_id(instance: &UiInstance, index: usize) -> super::focus::ComponentInstanceId {
    super::focus::ComponentInstanceId {
        definition: instance.definition,
        path: instance.layout.component_paths[index].clone(),
    }
}

fn scroll_to_target(
    state: &mut super::definition::UiDefinitionState,
    instance: &UiInstance,
    bounds: super::geometry::Rect,
) {
    let height = instance.settings.viewport.height;
    let delta = if bounds.y < 0.0 || bounds.height > height {
        bounds.y
    } else if bounds.y + bounds.height > height {
        bounds.y + bounds.height - height
    } else {
        0.0
    };
    let maximum = (instance.layout.content_height - height).max(0.0);
    state.scroll = (instance.layout.scroll_offset + delta).clamp(0.0, maximum);
}
