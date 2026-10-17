//! Routes targeted input against published props and retained physical state.
use super::UiRegistry;
use crate::{
    application::app::Application,
    events::interface::{ComponentInput, EventSink},
    ui::{
        display_list::RenderRevision,
        input::{ButtonState, Key, UiInput},
        instance::UiInstanceId,
    },
};
use pixui_base::{PixuiResult, pixui_error};

impl UiRegistry {
    pub(super) fn component_event(
        &mut self,
        id: UiInstanceId,
        index: usize,
        input: &ComponentInput,
        app: &Application,
    ) -> PixuiResult<(bool, crate::events::binding::InputActions)> {
        let instance = self
            .instances
            .get_mut(&id)
            .ok_or_else(|| pixui_error!("unknown UI instance"))?;
        let path = &instance.layout.component_paths[index];
        let address = instance.layout.component_addresses[index];
        let prepared = &instance.layout.events[index];
        let descriptor = app.components.resolve(address)?;
        if prepared.descriptors.is_empty()
            || matches!(input, ComponentInput::Activate)
                && prepared.bindings.iter().all(Option::is_none)
        {
            return Ok((false, vec![]));
        }
        if !descriptor.input_interest.accepts(input) {
            if matches!(input, ComponentInput::Activate) && prepared.has("activate") {
                let payload = pixui_reflect::DynamicObject::from_reflect(
                    crate::events::standard::ActivationEventsActivatePayload {},
                );
                return Ok((
                    false,
                    vec![prepared.call("activate", app, &payload).and_then(|call| {
                        call.ok_or_else(|| pixui_error!("activation became unbound"))
                    })],
                ));
            }
            return Ok((false, vec![]));
        }
        let sink = EventSink::new(
            prepared.descriptors,
            prepared.bindings.iter().map(Option::is_some).collect(),
        )?;
        // Hooks may mutate retained state even when no callback is emitted or
        // default handling continues. Geometry compatibility is checked on render.
        if !instance.dirty {
            instance.redraw_only = true;
        }
        instance.dirty = true;
        let state = instance.state.component_mut(path)?;
        if state.component_address() != Some(address) {
            return Err(pixui_error!("input component registration changed"));
        }
        let consumed = (descriptor.input)(prepared.props.as_ref(), state, input, &sink)?;
        // Legacy Rust parts can attach standard activation to any component that
        // declares ActivationEvents without implementing a custom input hook.
        if !consumed
            && matches!(input, ComponentInput::Activate)
            && let Some(index) = prepared.index("activate")
        {
            sink.emit(
                index,
                pixui_reflect::DynamicObject::from_reflect(
                    crate::events::standard::ActivationEventsActivatePayload {},
                ),
            )?;
        }
        let mut actions = vec![];
        for event in sink.into_pending() {
            let binding = prepared.bindings[event.index]
                .as_ref()
                .expect("only bound events queued");
            let name = prepared.descriptors[event.index].name;
            let action = binding(app, &event.payload)
                .map(|mut call| {
                    call.event_name = Some(name.into());
                    call
                })
                .map_err(|error| pixui_error!("event `{name}`: {error}"));
            let failed = action.is_err();
            actions.push(action);
            if failed {
                break;
            }
        }
        Ok((consumed, actions))
    }
    /// Tracks valid pointer gestures and lets custom hooks consume targeted input.
    /// Global shortcuts and text editing run before this policy.
    pub(super) fn route_component_input(
        &mut self,
        id: UiInstanceId,
        revision: RenderRevision,
        input: &UiInput,
        app: &Application,
    ) -> PixuiResult<bool> {
        if matches!(input, UiInput::Keyboard(event) if matches!(&event.key, Key::Named(name) if name == "Tab" || name == "F11"))
        {
            return Ok(false);
        }
        let instance = self.instance(id)?;
        if !instance.visible {
            return Ok(false);
        }
        let pointer = match input {
            UiInput::PointerMoved(point)
            | UiInput::MouseButton {
                position: point, ..
            } => Some(*point),
            UiInput::MouseWheel { .. } => instance.pointer_position,
            _ => None,
        };
        if pointer.is_some_and(|point| !point.x.is_finite() || !point.y.is_finite()) {
            return Err(pixui_error!("invalid pointer coordinates"));
        }
        let target = if let Some(point) = pointer {
            instance
                .layout
                .hit_regions
                .iter()
                .rev()
                .find(|region| region.bounds.contains(point))
                .map(|region| region.component_index)
        } else if matches!(input, UiInput::PointerLeft)
            && self.hover_sources.get(&instance.definition) == Some(&id)
        {
            self.definitions[&instance.definition]
                .state
                .hover
                .as_ref()
                .and_then(|hover| instance.layout.component_indices.get(hover.path()).copied())
        } else if matches!(
            input,
            UiInput::Keyboard(_)
                | UiInput::ModifiersChanged(_)
                | UiInput::ImeCommit(_)
                | UiInput::ImePreedit { .. }
        ) {
            self.definitions[&instance.definition]
                .state
                .focus
                .as_ref()
                .and_then(|focus| instance.layout.component_indices.get(focus.path()).copied())
        } else {
            None
        };
        if revision < instance.compatible_revision
            || revision > instance.revision
            || revision.0 == 0
            || instance.geometry_stale
        {
            let current = instance.revision;
            self.pointer_presses.remove(&id);
            if matches!(input, UiInput::PointerMoved(_) | UiInput::PointerLeft) {
                return Ok(false);
            }
            if target.is_some() || matches!(input, UiInput::MouseButton { .. }) {
                return Err(pixui_base::report(
                    crate::ui::input_error::StaleInputRevision {
                        presented: revision,
                        current,
                    },
                ));
            }
            return Ok(false);
        }
        if let UiInput::MouseButton {
            button,
            state: ButtonState::Pressed,
            ..
        } = input
        {
            self.pointer_presses.remove(&id);
            if let Some(index) = target {
                self.pointer_presses.insert(
                    id,
                    (
                        self.instances[&id].layout.component_paths[index].clone(),
                        *button,
                    ),
                );
            }
        }
        let click = if let UiInput::MouseButton {
            button,
            state: ButtonState::Released,
            position,
            modifiers,
        } = input
        {
            let pressed = self.pointer_presses.remove(&id);
            let matched = target.is_some_and(|index| {
                pressed.as_ref().is_some_and(|(path, pressed_button)| {
                    path == &self.instances[&id].layout.component_paths[index]
                        && pressed_button == button
                })
            });
            if !matched {
                return Ok(true);
            }
            Some(ComponentInput::Click {
                position: *position,
                button: *button,
                modifiers: *modifiers,
            })
        } else {
            None
        };
        let Some(index) = target else {
            return Ok(false);
        };
        let (mut consumed, mut actions) =
            self.component_event(id, index, &ComponentInput::Raw(input.clone()), app)?;
        if !consumed && let Some(click) = click {
            let (handled, emitted) = self.component_event(id, index, &click, app)?;
            consumed = handled;
            actions.extend(emitted);
        }
        if !actions.is_empty() || consumed {
            let definition = self.instances[&id].definition;
            self.dirty_definition(definition, false);
        }
        consumed |= actions.iter().any(Result::is_err);
        self.pending_actions.extend(actions);
        Ok(consumed)
    }
}
