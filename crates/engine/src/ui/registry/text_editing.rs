//! Focus-session editing, ordered native effects and shared selection ownership.
use super::{UiRegistry, next_id};
use crate::{
    application::{action::ActionCall, app::Application},
    live_model::identity::ComponentPath,
    ui::{
        display_list::RenderRevision,
        focus::ComponentInstanceId,
        geometry::Rect,
        input::{ButtonState, Key, MouseButton, UiInput},
        instance::UiInstanceId,
        text_input::{
            editing::{Direction, Edit, MAX_CONTENT_BYTES, Selection},
            protocol::{ClipboardReply, EditingSessionId, HostEffect, NativeTextInput},
        },
        window_properties::WindowCommand,
    },
};
use pixui_base::{PixuiResult, pixui_error};

pub(super) struct EditingSession {
    pub(super) id: EditingSessionId,
    instance: UiInstanceId,
    target: ComponentInstanceId,
    pub(super) origin: Option<RenderRevision>,
}
#[derive(Clone, Copy)]
enum ClipboardOperation {
    Copy,
    Cut,
    Paste,
}
pub(super) struct PendingClipboard {
    pub(super) session: EditingSessionId,
    target: ComponentInstanceId,
    instance: UiInstanceId,
    content_revision: u64,
    selection_revision: u64,
    operation: ClipboardOperation,
}
pub(super) struct PointerCapture {
    instance: UiInstanceId,
    path: ComponentPath,
    anchor: usize,
}

impl UiRegistry {
    pub(super) fn sync_text_sessions(
        &mut self,
        origin: Option<(UiInstanceId, RenderRevision)>,
        app: &Application,
    ) -> PixuiResult<()> {
        let definitions: Vec<_> = self.definitions.keys().copied().collect();
        for definition in definitions {
            let desired = self.definitions[&definition]
                .state
                .focus
                .clone()
                .and_then(|focus| {
                    let source = *self.focus_sources.get(&definition)?;
                    let instance = self.instances.get(&source)?;
                    (instance.visible
                        && instance.native_focused
                        && instance
                            .layout
                            .text_inputs
                            .iter()
                            .any(|target| target.path == *focus.path())
                        && instance
                            .layout
                            .focus_targets
                            .iter()
                            .any(|target| target.path == *focus.path()))
                    .then_some((source, focus))
                });
            let unchanged = match (self.text_sessions.get(&definition), desired.as_ref()) {
                (None, None) => true,
                (Some(session), Some((source, focus))) => {
                    session.instance == *source && session.target == *focus
                }
                _ => false,
            };
            if !unchanged {
                if let Some(old) = self.text_sessions.remove(&definition) {
                    if let Some(edit) = self
                        .definitions
                        .get_mut(&definition)
                        .expect("definition")
                        .state
                        .text_inputs
                        .get_mut(old.target.path())
                    {
                        edit.composing = false;
                    }
                    self.pending_clipboards
                        .retain(|_, request| request.session != old.id);
                }
                self.text_capture = self.text_capture.take().filter(|capture| {
                    self.instances
                        .get(&capture.instance)
                        .is_some_and(|instance| instance.definition != definition)
                        || desired.as_ref().is_some_and(|(source, focus)| {
                            capture.instance == *source && &capture.path == focus.path()
                        })
                });
                if let Some((source, target)) = desired {
                    let instance = &self.instances[&source];
                    let published = instance
                        .layout
                        .text_inputs
                        .iter()
                        .find(|input| &input.path == target.path())
                        .expect("input target");
                    let edit = self
                        .definitions
                        .get_mut(&definition)
                        .expect("definition")
                        .state
                        .text_inputs
                        .entry(target.path().clone())
                        .or_insert_with(|| published.state.clone());
                    edit.blink_reset_us = timestamp(app, instance.settings.timestamp_us);
                    self.text_sessions.insert(
                        definition,
                        EditingSession {
                            id: EditingSessionId(next_id()?),
                            instance: source,
                            target,
                            origin: origin
                                .filter(|(id, _)| *id == source)
                                .map(|(_, revision)| revision),
                        },
                    );
                }
            }
        }
        for (id, instance) in &mut self.instances {
            let mut native = NativeTextInput::default();
            if let Some(session) = self.text_sessions.get(&instance.definition)
                && session.instance == *id
                && let Some(target) = instance
                    .layout
                    .text_inputs
                    .iter()
                    .find(|target| &target.path == session.target.path())
            {
                let edit = self.definitions[&instance.definition]
                    .state
                    .text_inputs
                    .get(&target.path)
                    .unwrap_or(&target.state);
                native.session = Some(session.id);
                native.editable = target.change.is_some();
                let x = (target.bounds.x
                    + target.geometry.inset
                    + target.geometry.x(edit.selection.head)
                    - target.scroll)
                    .clamp(target.bounds.x, target.bounds.x + target.bounds.width);
                native.caret = Rect {
                    x,
                    y: target.bounds.y + (target.bounds.height - target.geometry.line_height) / 2.0,
                    width: 1.0,
                    height: target.geometry.line_height,
                }
                .intersect(target.clip);
                native.capture_pointer = self
                    .text_capture
                    .as_ref()
                    .is_some_and(|capture| capture.instance == *id);
            }
            if instance.native_text_input != native {
                instance
                    .outputs
                    .window_commands
                    .publish(WindowCommand::SetTextInput(native.clone()));
                instance.native_text_input = native;
            }
        }
        Ok(())
    }

    pub(super) fn text_keyboard(
        &mut self,
        id: UiInstanceId,
        revision: RenderRevision,
        token: Option<EditingSessionId>,
        dedicated: bool,
        input: &UiInput,
        app: &Application,
    ) -> PixuiResult<Option<Option<ActionCall>>> {
        if matches!(input, UiInput::Keyboard(event) if matches!(&event.key, Key::Named(name) if name == "F11"))
        {
            return Ok(None);
        }
        if !matches!(
            input,
            UiInput::Keyboard(_)
                | UiInput::ImeCommit(_)
                | UiInput::ImePreedit { .. }
                | UiInput::ImeEnabled
                | UiInput::ImeDisabled
        ) {
            return Ok(None);
        }
        let instance = self.instance(id)?;
        let definition = instance.definition;
        let Some(session) = self
            .text_sessions
            .get(&definition)
            .filter(|session| session.instance == id)
        else {
            if token.is_some() {
                return Err(pixui_error!("expired text input session"));
            }
            return Ok(None);
        };
        if dedicated {
            if token.is_some_and(|token| token != session.id)
                || (token.is_none() && Some(revision) != session.origin)
            {
                return Err(pixui_error!("stale text input session"));
            }
        } else if instance.geometry_stale
            || revision.0 == 0
            || revision < instance.compatible_revision
            || revision > instance.revision
        {
            return Err(pixui_error!("stale UI input revision"));
        }
        if instance.geometry_stale {
            return Err(pixui_error!("text input awaits successful preparation"));
        }
        let path = session.target.path().clone();
        if let UiInput::Keyboard(event) = input
            && matches!(&event.key, Key::Named(name) if name == "Tab")
        {
            if dedicated {
                let current = instance.revision;
                let result = self.input(id, current, input.clone(), app);
                if let Some(session) = self.text_sessions.get_mut(&definition) {
                    session.origin = Some(revision);
                }
                return result.map(Some);
            }
            return Ok(None);
        }
        let target = instance
            .layout
            .text_inputs
            .iter()
            .find(|target| target.path == path)
            .expect("session target");
        let editable = target.change.is_some();
        let initial_state = target.state.clone();
        let time = timestamp(app, instance.settings.timestamp_us);
        let edit = self
            .definitions
            .get_mut(&definition)
            .expect("definition")
            .state
            .text_inputs
            .entry(path.clone())
            .or_insert(initial_state);
        let operation = match input {
            UiInput::ImeEnabled => return Ok(Some(None)),
            UiInput::ImeDisabled => {
                edit.composing = false;
                return Ok(Some(None));
            }
            UiInput::ImePreedit { text, cursor } => {
                if text.len() > MAX_CONTENT_BYTES
                    || cursor.is_some_and(|(a, b)| {
                        a > text.len()
                            || b > text.len()
                            || !text.is_char_boundary(a)
                            || !text.is_char_boundary(b)
                    })
                {
                    return Err(pixui_error!("invalid IME preedit"));
                }
                edit.composing = !text.is_empty();
                return Ok(Some(None));
            }
            UiInput::ImeCommit(text) => {
                edit.composing = false;
                Operation::Edit(Edit::Insert(text.clone()))
            }
            UiInput::Keyboard(event) => {
                if event.state != ButtonState::Pressed || event.synthetic {
                    return Ok(Some(None));
                }
                match keyboard(event) {
                    Some(Operation::Clipboard(_)) if event.repeat => return Ok(Some(None)),
                    operation => {
                        if edit.composing {
                            return Ok(Some(None));
                        }
                        let Some(operation) = operation else {
                            return Ok(Some(None));
                        };
                        operation
                    }
                }
            }
            _ => return Ok(None),
        };
        match operation {
            Operation::Clipboard(operation) => {
                if matches!(
                    operation,
                    ClipboardOperation::Copy | ClipboardOperation::Cut
                ) && edit.selection.collapsed()
                {
                    return Ok(Some(None));
                }
                if !editable && !matches!(operation, ClipboardOperation::Copy) {
                    return Ok(Some(None));
                }
                self.clipboard_request(id, operation)?;
                Ok(Some(None))
            }
            Operation::Edit(operation) => {
                if !editable && matches!(operation, Edit::Insert(_) | Edit::Delete { .. }) {
                    return Ok(Some(None));
                }
                let proposal = edit.edit(operation, time)?;
                let action = if let Some(proposal) = proposal {
                    let content = proposal.content.clone();
                    edit.pending = Some(proposal);
                    let change = self.instances[&id]
                        .layout
                        .text_inputs
                        .iter()
                        .find(|target| target.path == path)
                        .and_then(|target| target.change.as_ref())
                        .expect("editable target");
                    match change(app, content) {
                        Ok(action) => Some(action),
                        Err(error) => {
                            self.definitions
                                .get_mut(&definition)
                                .expect("definition")
                                .state
                                .text_inputs
                                .get_mut(&path)
                                .expect("edit")
                                .pending = None;
                            return Err(error);
                        }
                    }
                } else {
                    None
                };
                self.dirty_definition(definition, false);
                Ok(Some(action))
            }
        }
    }

    pub(super) fn captured_text_input(
        &mut self,
        id: UiInstanceId,
        revision: RenderRevision,
        token: Option<EditingSessionId>,
        input: &UiInput,
        app: &Application,
    ) -> PixuiResult<bool> {
        if !self
            .text_capture
            .as_ref()
            .is_some_and(|capture| capture.instance == id)
            || !matches!(
                input,
                UiInput::PointerMoved(_)
                    | UiInput::MouseButton {
                        button: MouseButton::Left,
                        state: ButtonState::Released,
                        ..
                    }
            )
        {
            return Ok(false);
        }
        let instance = self.instance(id)?;
        let session = self
            .text_sessions
            .get(&instance.definition)
            .ok_or_else(|| pixui_error!("expired selection session"))?;
        if instance.geometry_stale
            || token.is_some_and(|token| token != session.id)
            || (token.is_none() && Some(revision) != session.origin)
        {
            return Err(pixui_error!("stale selection session"));
        }
        // A captured gesture stays on this target as its own horizontal scroll changes.
        self.text_pointer(id, input, app)?;
        Ok(true)
    }

    pub(super) fn release_text_capture(
        &mut self,
        id: UiInstanceId,
        input: &UiInput,
        app: &Application,
    ) -> PixuiResult<bool> {
        if self
            .text_capture
            .as_ref()
            .is_some_and(|capture| capture.instance == id)
            && matches!(
                input,
                UiInput::MouseButton {
                    button: MouseButton::Left,
                    state: ButtonState::Released,
                    ..
                }
            )
        {
            self.text_pointer(id, input, app)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn text_pointer(
        &mut self,
        id: UiInstanceId,
        input: &UiInput,
        app: &Application,
    ) -> PixuiResult<()> {
        let instance = self.instance(id)?;
        let definition = instance.definition;
        let (point, extend, press) = match input {
            UiInput::MouseButton {
                button: MouseButton::Left,
                state: ButtonState::Pressed,
                position,
                modifiers,
            } => (*position, modifiers.shift, true),
            UiInput::PointerMoved(point)
                if self
                    .text_capture
                    .as_ref()
                    .is_some_and(|capture| capture.instance == id) =>
            {
                (*point, true, false)
            }
            UiInput::MouseButton {
                button: MouseButton::Left,
                state: ButtonState::Released,
                ..
            } => {
                self.text_capture = None;
                self.sync_text_sessions(None, app)?;
                return Ok(());
            }
            _ => return Ok(()),
        };
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(pixui_error!("invalid selection coordinates"));
        }
        let path = if press {
            self.definitions[&definition]
                .state
                .focus
                .as_ref()
                .map(|focus| focus.path().clone())
        } else {
            self.text_capture
                .as_ref()
                .map(|capture| capture.path.clone())
        };
        let Some(target) = path.and_then(|path| {
            instance
                .layout
                .text_inputs
                .iter()
                .find(|target| target.path == path)
        }) else {
            return Ok(());
        };
        let head = target
            .geometry
            .nearest(point.x - target.bounds.x - target.geometry.inset + target.scroll);
        let time = timestamp(app, instance.settings.timestamp_us);
        let path = target.path.clone();
        let initial_state = target.state.clone();
        let edit = self
            .definitions
            .get_mut(&definition)
            .expect("definition")
            .state
            .text_inputs
            .entry(path.clone())
            .or_insert(initial_state);
        let anchor = if press {
            if extend { edit.selection.anchor } else { head }
        } else {
            self.text_capture.as_ref().expect("captured pointer").anchor
        };
        edit.select(Selection { anchor, head }, time)?;
        self.text_capture = Some(PointerCapture {
            instance: id,
            path,
            anchor,
        });
        self.dirty_definition(definition, false);
        self.sync_text_sessions(None, app)?;
        Ok(())
    }

    fn clipboard_request(
        &mut self,
        id: UiInstanceId,
        operation: ClipboardOperation,
    ) -> PixuiResult<()> {
        let instance = self.instance(id)?;
        if !instance.clipboard_available {
            return Err(pixui_error!(
                "clipboard unavailable: attach a native host or test adapter"
            ));
        }
        if self.pending_clipboards.len() >= 16 {
            return Err(pixui_error!("too many pending clipboard requests"));
        }
        let definition = instance.definition;
        let session = &self.text_sessions[&definition];
        let state = &self.definitions[&definition].state.text_inputs[session.target.path()];
        let request = next_id()?;
        let effect = match operation {
            ClipboardOperation::Paste => HostEffect::ReadClipboard { request },
            _ => HostEffect::WriteClipboard {
                request,
                text: state.content[state.selection.range()].to_owned(),
            },
        };
        instance.outputs.effect(effect)?;
        self.pending_clipboards.insert(
            request,
            PendingClipboard {
                session: session.id,
                target: session.target.clone(),
                instance: id,
                content_revision: state.content_revision,
                selection_revision: state.selection_revision,
                operation,
            },
        );
        Ok(())
    }

    pub(super) fn clipboard_reply(
        &mut self,
        reply: ClipboardReply,
        app: &Application,
    ) -> PixuiResult<Option<ActionCall>> {
        let Some(pending) = self.pending_clipboards.get(&reply.request) else {
            return Ok(None);
        };
        if pending.instance != reply.instance {
            return Err(pixui_error!("clipboard reply belongs to another instance"));
        }
        let pending = self
            .pending_clipboards
            .remove(&reply.request)
            .expect("checked pending reply");
        let definition = pending.target.definition();
        if !self
            .text_sessions
            .get(&definition)
            .is_some_and(|session| session.id == pending.session)
        {
            return Ok(None);
        }
        let Some(state) = self.definitions[&definition]
            .state
            .text_inputs
            .get(pending.target.path())
        else {
            return Ok(None);
        };
        if state.content_revision != pending.content_revision
            || state.selection_revision != pending.selection_revision
        {
            return Ok(None);
        }
        let result = reply
            .result
            .map_err(|error| pixui_error!("clipboard: {error}"))?;
        let edit = match pending.operation {
            ClipboardOperation::Copy => return Ok(None),
            ClipboardOperation::Cut => Edit::Insert(String::new()),
            ClipboardOperation::Paste => match result {
                Some(text) => Edit::Insert(text),
                None => return Ok(None),
            },
        };
        let instance = &self.instances[&reply.instance];
        let time = timestamp(app, instance.settings.timestamp_us);
        let state = self
            .definitions
            .get_mut(&definition)
            .expect("definition")
            .state
            .text_inputs
            .get_mut(pending.target.path())
            .expect("edit");
        let Some(proposal) = state.edit(edit, time)? else {
            return Ok(None);
        };
        let content = proposal.content.clone();
        state.pending = Some(proposal);
        let target = instance
            .layout
            .text_inputs
            .iter()
            .find(|target| &target.path == pending.target.path())
            .expect("session input");
        let action =
            target
                .change
                .as_ref()
                .ok_or_else(|| pixui_error!("input became read-only"))?(app, content);
        if action.is_err() {
            state.pending = None;
        }
        self.dirty_definition(definition, false);
        action.map(Some)
    }
}
fn timestamp(app: &Application, override_us: Option<u64>) -> u64 {
    override_us.unwrap_or_else(|| app.render_clock.timestamp_us())
}
enum Operation {
    Edit(Edit),
    Clipboard(ClipboardOperation),
}
fn keyboard(event: &crate::ui::input::KeyboardEvent) -> Option<Operation> {
    let primary = if cfg!(target_os = "macos") {
        event.modifiers.super_key
    } else {
        event.modifiers.control
    };
    if primary
        && !event.modifiers.alt
        && let Key::Character(key) = &event.key
    {
        return match key.to_ascii_lowercase().as_str() {
            "a" => Some(Operation::Edit(Edit::SelectAll)),
            "c" => Some(Operation::Clipboard(ClipboardOperation::Copy)),
            "x" => Some(Operation::Clipboard(ClipboardOperation::Cut)),
            "v" => Some(Operation::Clipboard(ClipboardOperation::Paste)),
            _ => None,
        };
    }
    let word = if cfg!(target_os = "macos") {
        event.modifiers.alt
    } else {
        event.modifiers.control
    };
    if let Key::Named(key) = &event.key {
        let direction = match key.as_str() {
            "ArrowLeft" if cfg!(target_os = "macos") && primary => Some(Direction::Start),
            "ArrowRight" if cfg!(target_os = "macos") && primary => Some(Direction::End),
            "ArrowLeft" => Some(Direction::Left),
            "ArrowRight" => Some(Direction::Right),
            "Home" => Some(Direction::Start),
            "End" => Some(Direction::End),
            _ => None,
        };
        if let Some(direction) = direction {
            return Some(Operation::Edit(Edit::Move {
                direction,
                extend: event.modifiers.shift,
                word,
            }));
        }
        match key.as_str() {
            "Backspace" => {
                return Some(Operation::Edit(Edit::Delete {
                    backwards: true,
                    word,
                }));
            }
            "Delete" => {
                return Some(Operation::Edit(Edit::Delete {
                    backwards: false,
                    word,
                }));
            }
            "Enter" => return None,
            _ => {}
        }
    }
    if primary && !event.modifiers.alt {
        return None;
    }
    event
        .text
        .as_ref()
        .map(|text| Operation::Edit(Edit::Insert(text.clone())))
}
