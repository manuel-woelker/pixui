//! Application-owned definitions, instances, revision-aware input, and rendering.

use super::{
    definition::{UiDefinition, UiDefinitionId},
    display_list::{RenderOutput, RenderRevision},
    input::{UiCommand, UiInput},
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
                geometry_stale: true,
                outputs,
                error: None,
                animation_request: None,
                visible: true,
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
        ) {
            instance.redraw_only = false;
        }
        match command {
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
            }
            UiCommand::Present { settings, .. } => {
                settings.validate()?;
                instance.settings = settings;
                instance.dirty = true;
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
        let instance = self
            .instances
            .get(&id)
            .ok_or_else(|| pixui_error!("unknown UI instance"))?;
        if revision < instance.compatible_revision
            || revision > instance.revision
            || revision.0 == 0
            || instance.geometry_stale
        {
            if matches!(input, UiInput::PointerMoved(_)) {
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
        match input {
            UiInput::PointerMoved(point) => {
                if !point.x.is_finite() || !point.y.is_finite() {
                    return Err(pixui_error!("invalid pointer coordinates"));
                }
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
            UiInput::Activate(point) => {
                if !point.x.is_finite() || !point.y.is_finite() {
                    return Err(pixui_error!("invalid pointer coordinates"));
                }
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
            UiInput::ActivateFocused => {
                let region = instance
                    .layout
                    .hit_regions
                    .iter()
                    .find(|region| Some(region.component_index) == state.focus);
                action = region
                    .map(|region| (region.activate)(application))
                    .transpose()?;
            }
            UiInput::FocusNext => {
                let regions = &instance.layout.hit_regions;
                let next = regions
                    .iter()
                    .position(|region| Some(region.component_index) == state.focus)
                    .map_or(0, |index| (index + 1) % regions.len());
                state.focus = regions.get(next).map(|region| region.component_index);
            }
            UiInput::Scroll(delta) => {
                if !delta.is_finite() {
                    return Err(pixui_error!("invalid scroll delta"));
                }
                let maximum =
                    (instance.layout.content_height - instance.settings.viewport.height).max(0.0);
                state.scroll = (state.scroll + delta).clamp(0.0, maximum);
                geometry_changed = state.scroll != before.scroll;
            }
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
            }
        }
        Ok(action)
    }

    /// Publishes only complete successful renders, preserving the last good
    /// revision and geometry on failure. Errors remain inspectable on the worker.
    /// Dropped consumers release their instance on the next rendering pass.
    pub fn render_dirty(&mut self, application: &Application) {
        self.instances
            .retain(|_, instance| instance.outputs.connected());
        for (id, instance) in &mut self.instances {
            if !instance.dirty || !instance.visible {
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
                    instance.revision = RenderRevision(revision);
                    instance.geometry_stale = false;
                    instance.error = None;
                    instance.outputs.publish(RenderOutput {
                        instance_id: *id,
                        revision: instance.revision,
                        display_list,
                        redraw_after,
                        timings,
                        animating,
                        animation_request: instance.animation_request,
                    });
                }
                Err(error) => instance.error = Some(format!("{error:?}")),
            }
        }
    }
}
