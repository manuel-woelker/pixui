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
                focus: None,
                hover: None,
                scroll: 0.0,
                dirty: true,
                geometry_stale: true,
                outputs,
                error: None,
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
        for instance in self.instances.values_mut() {
            instance.dirty = true;
            instance.redraw_only = false;
            instance.geometry_stale = true;
            instance.focus = None;
            instance.hover = None;
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
        let instance = self
            .instances
            .get_mut(&command.instance())
            .ok_or_else(|| pixui_error!("unknown UI instance"))?;
        if !matches!(command, UiCommand::Redraw { .. }) {
            instance.redraw_only = false;
        }
        match command {
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
            UiCommand::Input {
                revision, input, ..
            } => {
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
                let target = match input {
                    UiInput::Activate(point) | UiInput::PointerMoved(point) => {
                        if !point.x.is_finite() || !point.y.is_finite() {
                            return Err(pixui_error!("invalid pointer coordinates"));
                        }
                        instance
                            .layout
                            .hit_regions
                            .iter()
                            .rposition(|region| region.bounds.contains(point))
                    }
                    UiInput::ActivateFocused => instance.focus,
                    _ => None,
                };
                match input {
                    UiInput::Activate(_) | UiInput::ActivateFocused => {
                        instance.focus = target;
                        instance.dirty = true;
                        if let Some(index) = target {
                            return (instance.layout.hit_regions[index].activate)(application)
                                .map(Some);
                        }
                    }
                    UiInput::PointerMoved(_) => {
                        if instance.hover != target {
                            instance.hover = target;
                            instance.dirty = true;
                        }
                    }
                    UiInput::FocusNext => {
                        let count = instance.layout.hit_regions.len();
                        instance.focus = if count == 0 {
                            None
                        } else {
                            Some(instance.focus.map_or(0, |index| (index + 1) % count))
                        };
                        instance.dirty = true;
                    }
                    UiInput::Scroll(delta) => {
                        if !delta.is_finite() {
                            return Err(pixui_error!("invalid scroll delta"));
                        }
                        instance.scroll = (instance.scroll + delta).max(0.0);
                        instance.dirty = true;
                        instance.geometry_stale = true;
                    }
                }
            }
            UiCommand::Close { .. } => unreachable!(),
        }
        Ok(None)
    }

    /// Publishes only complete successful renders, preserving the last good
    /// revision and geometry on failure. Errors remain inspectable on the worker.
    /// Dropped consumers release their instance on the next rendering pass.
    pub fn render_dirty(&mut self, application: &Application) {
        self.instances
            .retain(|_, instance| instance.outputs.connected());
        for (id, instance) in &mut self.instances {
            if !instance.dirty {
                continue;
            }
            instance.dirty = false;
            let Some(revision) = instance.revision.0.checked_add(1) else {
                instance.error = Some("render revision exhausted".into());
                continue;
            };
            let result = renderer::render(
                &self.definitions[&instance.definition].template,
                &mut instance.state,
                application,
                &instance.settings,
                instance.scroll,
                instance.focus,
                instance.hover,
            );
            match result {
                Ok((display_list, mut layout, scroll, redraw_after)) => {
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
                    instance.scroll = scroll;
                    instance.revision = RenderRevision(revision);
                    instance.geometry_stale = false;
                    instance.error = None;
                    instance.outputs.publish(RenderOutput {
                        instance_id: *id,
                        revision: instance.revision,
                        display_list,
                        redraw_after,
                    });
                }
                Err(error) => instance.error = Some(format!("{error:?}")),
            }
        }
    }
}
