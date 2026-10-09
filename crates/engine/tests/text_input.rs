//! Controlled editing through the actual application worker, without a display server.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::{
        app::Application, application_handle::ApplicationHandle,
        application_slice::ApplicationSlice, entity_mut::EntityMut,
    },
    components::text_input::TextInputProps,
    expression::{context::ExpressionContext, expression::Expression},
    layout::{container::ContainerPart, style::LayoutStyle},
    live_model::part::ComponentPart,
    ui::{
        definition::{UiDefinition, UiDefinitionId},
        display_list::{DrawCommand, RenderRevision},
        geometry::{Point, Size},
        input::{ButtonState, Key, KeyboardEvent, Modifiers, MouseButton, UiCommand, UiInput},
        instance::UiInstanceId,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
        text_input::{
            binding::ChangeBinding,
            editing::{EditState, MAX_CONTENT_BYTES},
            protocol::{ClipboardReply, EditingSessionId, HostEffect},
        },
        window_properties::WindowCommand,
    },
};
use std::time::Duration;

#[pixui_engine::application::action::slice_actions(slice = "input", facade = Actions)]
mod actions {
    use super::*;
    #[action]
    pub fn change(mut content: EntityMut<String>, value: String) {
        *content = value;
    }
    #[action]
    pub fn normalize(mut content: EntityMut<String>, value: String) {
        *content = value.to_uppercase();
    }
    #[action]
    pub fn reject(_value: String) -> PixuiResult<()> {
        Err(pixui_error!("rejected edit"))
    }
    #[action]
    pub fn ignore(_value: String) {}
    #[action]
    pub fn fail_after_change(mut content: EntityMut<String>, value: String) -> PixuiResult<()> {
        *content = value;
        Err(pixui_error!("changed before error"))
    }
}
fn change<const MODE: u8>(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ChangeBinding> {
    let action = context
        .application()?
        .slice_named("input")?
        .action_handle_named(match MODE {
            1 => "normalize",
            2 => "reject",
            3 => "ignore",
            4 => "fail_after_change",
            _ => "change",
        })?;
    Ok(Box::new(move |_, value| action.call(vec![Box::new(value)])))
}
struct Fixture {
    app: ApplicationHandle,
    actions: actions::Actions,
    definition: UiDefinitionId,
    instance: UiInstanceId,
    outputs: OutputReceiver,
    revision: RenderRevision,
}
impl Fixture {
    fn new<const MODE: u8>(initial: &str, readonly: bool) -> Self {
        let app = Application::new();
        let mut slice = ApplicationSlice::new("input");
        slice.bind("content", initial.to_owned()).unwrap();
        let slice = app.add_slice(slice).unwrap();
        actions::Actions::register(&app, slice).unwrap();
        let actions = actions::Actions::bind(&app).unwrap();
        let components = app.register_standard_components().unwrap();
        app.register_standard_painters().unwrap();
        let reference = app.entity_ref::<String>(slice, "content").unwrap();
        let mut input = ComponentPart::typed_with_expressions(
            components.text_input,
            vec![Expression::entity(reference)],
            |_, _, values| {
                Ok(TextInputProps {
                    content: values[0].downcast_ref::<String>().unwrap().clone(),
                })
            },
        )
        .with_layout(LayoutStyle::fixed(120.0, 40.0));
        if !readonly {
            input = input.with_change(change::<MODE>);
        }
        let definition = app
            .register_ui(UiDefinition::new(
                "input",
                ContainerPart::column()
                    .with_children(vec![input.clone().into(), input.into()])
                    .into(),
            ))
            .unwrap();
        let (instance, outputs) = app.create_ui(definition, settings(400.0, 1.0)).unwrap();
        let revision = outputs
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .revision;
        Self {
            app,
            actions,
            definition,
            instance,
            outputs,
            revision,
        }
    }
    fn focus(&self, index: usize) {
        let instance = self.instance;
        let target = self
            .app
            .inspect(move |app| {
                let instance = app.uis().instance(instance)?;
                instance.component_id(instance.layout().text_inputs[index].component_index)
            })
            .unwrap();
        self.app
            .ui_command(UiCommand::Focus {
                instance,
                target: Some(target),
            })
            .unwrap();
    }
    fn session(&self) -> EditingSessionId {
        self.app.inspect(|_| Ok(())).unwrap();
        let mut session = None;
        while let Ok(command) = self.outputs.window_commands().try_recv() {
            if let WindowCommand::SetTextInput(input) = command {
                session = input.session;
            }
        }
        session.expect("editing session published")
    }
    fn send(&self, session: Option<EditingSessionId>, input: UiInput) -> PixuiResult<()> {
        self.app.ui_command(UiCommand::TextInput {
            instance: self.instance,
            revision: self.revision,
            session,
            input: Box::new(input),
        })
    }
    fn state(&self, index: usize) -> EditState {
        let instance = self.instance;
        let definition = self.definition;
        self.app
            .inspect(move |app| {
                let target = &app.uis().instance(instance)?.layout().text_inputs[index];
                Ok(app.uis().definition(definition)?.state().text_inputs[&target.path].clone())
            })
            .unwrap()
    }
    fn text(&self, session: Option<EditingSessionId>, text: &str) -> PixuiResult<()> {
        let mut key = KeyboardEvent::named("text");
        key.key = Key::Character(text.into());
        key.text = Some(text.into());
        self.send(session, UiInput::Keyboard(key))
    }
    fn named(&self, session: EditingSessionId, name: &str, shift: bool) {
        let mut key = KeyboardEvent::named(name);
        key.modifiers.shift = shift;
        self.send(Some(session), UiInput::Keyboard(key)).unwrap();
    }
    fn shortcut(&self, session: EditingSessionId, name: &str) -> PixuiResult<()> {
        let mut key = KeyboardEvent::named("shortcut");
        key.key = Key::Character(name.into());
        if cfg!(target_os = "macos") {
            key.modifiers.super_key = true;
        } else {
            key.modifiers.control = true;
        }
        self.send(Some(session), UiInput::Keyboard(key))
    }
    fn attach(&self) {
        self.app
            .ui_command(UiCommand::HostAttached {
                instance: self.instance,
                clipboard: true,
            })
            .unwrap();
    }
    fn effect(&self) -> HostEffect {
        self.outputs
            .effects()
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
    }
    fn reply(&self, effect: HostEffect, result: Result<Option<String>, String>) -> PixuiResult<()> {
        let request = match effect {
            HostEffect::ReadClipboard { request } | HostEffect::WriteClipboard { request, .. } => {
                request
            }
        };
        self.app.ui_command(UiCommand::Clipboard(ClipboardReply {
            instance: self.instance,
            request,
            result,
        }))
    }
}
fn settings(width: f32, scale: f32) -> PresentationSettings {
    PresentationSettings {
        viewport: Size {
            width,
            height: 200.0,
        },
        scale_factor: scale,
        timestamp_us: Some(0),
        ..Default::default()
    }
}

#[test]
fn rapid_typing_and_click_handshake_use_authoritative_values() {
    let f = Fixture::new::<0>("", false);
    // Click and several text events can share one native presented revision.
    let id = f.instance;
    let point = f
        .app
        .inspect(move |app| {
            let bounds = app.uis().instance(id)?.layout().text_inputs[0].bounds;
            Ok(Point {
                x: bounds.x + 10.0,
                y: bounds.y + 10.0,
            })
        })
        .unwrap();
    f.app
        .ui_command(UiCommand::Input {
            instance: f.instance,
            revision: f.revision,
            input: UiInput::MouseButton {
                button: MouseButton::Left,
                state: ButtonState::Pressed,
                position: point,
                modifiers: Modifiers::default(),
            },
        })
        .unwrap();
    f.text(None, "a").unwrap();
    f.text(None, "é").unwrap();
    f.text(None, "👩‍💻").unwrap();
    let session = f.session();
    f.text(Some(session), "z").unwrap();
    assert_eq!(f.state(0).content, "aé👩‍💻z");
    assert_eq!(f.state(0).selection.head, "aé👩‍💻z".len());
    f.named(session, "ArrowLeft", false);
    f.named(session, "Backspace", false);
    assert_eq!(f.state(0).content, "aéz");
    // Repeats insert normally; synthetic keys and releases do not.
    let mut key = KeyboardEvent::named("repeat");
    key.text = Some("r".into());
    key.repeat = true;
    f.send(Some(session), UiInput::Keyboard(key.clone()))
        .unwrap();
    key.synthetic = true;
    f.send(Some(session), UiInput::Keyboard(key.clone()))
        .unwrap();
    key.synthetic = false;
    key.state = ButtonState::Released;
    f.send(Some(session), UiInput::Keyboard(key)).unwrap();
    assert_eq!(f.state(0).content, "aérz");
    // Old generic pointer input does not acquire the editing session's privilege.
    assert!(
        f.app
            .ui_command(UiCommand::Input {
                instance: f.instance,
                revision: f.revision,
                input: UiInput::MouseButton {
                    button: MouseButton::Left,
                    state: ButtonState::Pressed,
                    position: Point { x: 10.0, y: 60.0 },
                    modifiers: Modifiers::default()
                }
            })
            .is_err()
    );
}

#[test]
fn normalization_rejection_noops_and_errors_preserve_the_controlled_contract() {
    let normal = Fixture::new::<1>("", false);
    normal.focus(0);
    let session = normal.session();
    normal.text(Some(session), "ß").unwrap();
    assert_eq!(normal.state(0).content, "SS");
    assert_eq!(normal.state(0).selection.head, 2);
    normal.actions.change("é").unwrap(); // External replacement clamps the caret.
    assert_eq!(normal.state(0).selection.head, 2);
    for mode in [2, 3] {
        let f = if mode == 2 {
            Fixture::new::<2>("old", false)
        } else {
            Fixture::new::<3>("old", false)
        };
        f.focus(0);
        let session = f.session();
        assert_eq!(f.text(Some(session), "x").is_err(), mode == 2);
        assert_eq!(f.state(0).content, "old");
        assert_eq!(f.state(0).selection.head, 0);
        // No-op deletion doesn't even call the deliberately failing action.
        f.named(session, "Backspace", false);
    }
    let f = Fixture::new::<4>("", false);
    f.focus(0);
    let session = f.session();
    assert!(f.text(Some(session), "actual").is_err());
    assert_eq!(f.state(0).content, "actual"); // Dispatch errors don't roll back mutations.
}

#[test]
fn readonly_selection_copy_and_ordered_clipboard_acknowledgements() {
    let f = Fixture::new::<0>("abc", true);
    f.focus(0);
    let session = f.session();
    f.text(Some(session), "x").unwrap();
    assert_eq!(f.state(0).content, "abc");
    f.shortcut(session, "a").unwrap();
    assert_eq!(f.state(0).selection.range(), 0..3);
    assert!(f.shortcut(session, "c").is_err()); // Headless host is unavailable by default.
    f.attach();
    f.shortcut(session, "c").unwrap();
    let effect = f.effect();
    assert!(matches!(&effect, HostEffect::WriteClipboard { text, .. } if text == "abc"));
    f.reply(effect, Ok(None)).unwrap();
    f.shortcut(session, "x").unwrap();
    f.shortcut(session, "v").unwrap();
    assert!(f.outputs.effects().try_recv().is_err());

    let f = Fixture::new::<0>("abc", false);
    f.focus(0);
    let session = f.session();
    f.attach();
    f.shortcut(session, "a").unwrap();
    f.shortcut(session, "x").unwrap();
    assert_eq!(f.state(0).content, "abc");
    assert!(f.reply(f.effect(), Err("write failed".into())).is_err());
    assert_eq!(f.state(0).content, "abc");
    f.shortcut(session, "x").unwrap();
    f.reply(f.effect(), Ok(None)).unwrap();
    assert_eq!(f.state(0).content, "");
    f.shortcut(session, "v").unwrap();
    f.reply(f.effect(), Ok(Some("x\r\n\ty\u{7}".into())))
        .unwrap();
    assert_eq!(f.state(0).content, "x y");
    f.shortcut(session, "v").unwrap();
    f.reply(f.effect(), Ok(None)).unwrap();
    f.shortcut(session, "v").unwrap();
    assert!(f.reply(f.effect(), Err("read failed".into())).is_err());
    f.shortcut(session, "v").unwrap();
    assert!(
        f.reply(f.effect(), Ok(Some("x".repeat(MAX_CONTENT_BYTES + 1))))
            .is_err()
    );
    assert_eq!(f.state(0).content, "x y");
}

#[test]
fn delayed_clipboard_results_are_cancelled_by_selection_content_focus_and_close() {
    let f = Fixture::new::<0>("abc", false);
    f.focus(0);
    let session = f.session();
    f.attach();
    f.shortcut(session, "v").unwrap();
    let effect = f.effect();
    f.named(session, "End", false);
    f.reply(effect, Ok(Some("stale".into()))).unwrap();
    assert_eq!(f.state(0).content, "abc");
    f.shortcut(session, "v").unwrap();
    let effect = f.effect();
    f.actions.change("external").unwrap();
    f.reply(effect, Ok(Some("stale".into()))).unwrap();
    assert_eq!(f.state(0).content, "external");
    f.shortcut(session, "v").unwrap();
    let effect = f.effect();
    f.focus(1);
    assert!(f.text(Some(session), "wrong target").is_err());
    f.reply(effect, Ok(Some("stale".into()))).unwrap();
    assert_eq!(f.state(1).content, "external");
    let session = f.session();
    f.shortcut(session, "v").unwrap();
    let effect = f.effect();
    f.app
        .ui_command(UiCommand::Close {
            instance: f.instance,
        })
        .unwrap();
    f.reply(effect, Ok(Some("stale".into()))).unwrap();
}

#[test]
fn ime_preedit_keeps_props_visible_and_commit_inserts_once() {
    let f = Fixture::new::<0>("", false);
    f.focus(0);
    let session = f.session();
    f.send(
        Some(session),
        UiInput::ImePreedit {
            text: "に".into(),
            cursor: Some((3, 3)),
        },
    )
    .unwrap();
    f.text(Some(session), "duplicate").unwrap();
    assert!(f.state(0).content.is_empty());
    f.send(Some(session), UiInput::ImeCommit("日本".into()))
        .unwrap();
    assert_eq!(f.state(0).content, "日本");
    assert!(!f.state(0).composing);
    assert!(
        f.send(
            Some(session),
            UiInput::ImePreedit {
                text: "é".into(),
                cursor: Some((1, 1))
            }
        )
        .is_err()
    );
    f.send(
        Some(session),
        UiInput::ImePreedit {
            text: "x".into(),
            cursor: None,
        },
    )
    .unwrap();
    f.actions.change("external").unwrap();
    assert!(!f.state(0).composing);
    assert!(
        f.send(Some(session), UiInput::ImeCommit("late".into()))
            .is_err()
    );
    let session = f.session();
    f.send(Some(session), UiInput::Focused(false)).unwrap();
    assert!(f.text(Some(session), "inactive").is_err());
}

#[test]
fn shared_selection_has_independent_scroll_and_capture_preserves_focus_outside_field() {
    let f = Fixture::new::<0>("abcdefghijklmnopqrstuv", false);
    let unrelated = f
        .app
        .register_ui(UiDefinition::new(
            "unrelated",
            pixui_engine::live_model::part::LivePart::Composite(
                pixui_engine::live_model::part::CompositePart { parts: vec![] },
            ),
        ))
        .unwrap();
    let (_other, _other_outputs) = f.app.create_ui(unrelated, settings(100.0, 1.0)).unwrap();

    f.focus(0);
    let session = f.session();
    let (peer, outputs) = f.app.create_ui(f.definition, settings(300.0, 2.0)).unwrap();
    outputs.recv_timeout(Duration::from_secs(3)).unwrap();
    f.named(session, "End", false);
    let id = f.instance;
    let (first, second, caret, point) = f
        .app
        .inspect(move |app| {
            let first = &app.uis().instance(id)?.layout().text_inputs[0];
            let second = &app.uis().instance(peer)?.layout().text_inputs[0];
            Ok((
                first.state.selection,
                second.state.selection,
                first.geometry.x(first.state.selection.head),
                Point {
                    x: first.bounds.x + first.geometry.inset,
                    y: first.bounds.y + 10.0,
                },
            ))
        })
        .unwrap();
    assert_eq!(first, second);
    assert!(caret > 120.0);
    // Use current geometry for the press, then the same session for captured motion.
    let revision = f
        .app
        .inspect(move |app| Ok(app.uis().instance(id)?.revision()))
        .unwrap();
    f.app
        .ui_command(UiCommand::Input {
            instance: id,
            revision,
            input: UiInput::MouseButton {
                button: MouseButton::Left,
                state: ButtonState::Pressed,
                position: point,
                modifiers: Modifiers::default(),
            },
        })
        .unwrap();
    f.send(
        Some(session),
        UiInput::PointerMoved(Point {
            x: 1000.0,
            y: 500.0,
        }),
    )
    .unwrap();
    f.send(
        Some(session),
        UiInput::MouseButton {
            button: MouseButton::Left,
            state: ButtonState::Released,
            position: Point {
                x: 1000.0,
                y: 500.0,
            },
            modifiers: Modifiers::default(),
        },
    )
    .unwrap();
    assert!(!f.state(0).selection.collapsed());
    f.text(Some(session), "replacement").unwrap();
    let output = f.outputs.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(output.display_list.commands.iter().any(|command| matches!(command, DrawCommand::DrawText { text, .. } if text.contains("replacement"))));
    // Frozen timestamps do not schedule continuous caret redraws.
    assert!(!output.animating);
}

#[test]
fn tab_handshake_transfers_session_and_native_focus_transfers_editing_ownership() {
    let f = Fixture::new::<0>("", false);
    f.focus(0);
    let old = f.session();
    f.text(Some(old), "a").unwrap();
    f.named(old, "Tab", false);
    f.text(None, "b").unwrap(); // Same presented revision, ordered after Tab.
    assert_eq!(f.state(1).content, "ba");
    assert!(f.text(Some(old), "stale").is_err());
    let new = f.session();
    let (peer, outputs) = f.app.create_ui(f.definition, settings(300.0, 1.5)).unwrap();
    outputs.recv_timeout(Duration::from_secs(3)).unwrap();
    f.send(Some(new), UiInput::Focused(false)).unwrap();
    f.app
        .ui_command(UiCommand::Input {
            instance: peer,
            revision: RenderRevision(0),
            input: UiInput::Focused(true),
        })
        .unwrap();
    f.app.inspect(|_| Ok(())).unwrap();
    let mut peer_session = None;
    while let Ok(command) = outputs.window_commands().try_recv() {
        if let WindowCommand::SetTextInput(input) = command {
            peer_session = input.session;
        }
    }
    f.app
        .ui_command(UiCommand::TextInput {
            instance: peer,
            revision: RenderRevision(0),
            session: peer_session,
            input: Box::new(UiInput::ImeCommit("peer".into())),
        })
        .unwrap();
    assert_eq!(f.state(1).content, "bpeera");
    assert!(f.text(Some(new), "inactive").is_err());
}

#[test]
fn clipboard_requests_are_bounded_and_foreign_replies_cannot_consume_them() {
    let f = Fixture::new::<0>("", false);
    f.focus(0);
    let session = f.session();
    f.attach();
    for _ in 0..16 {
        f.shortcut(session, "v").unwrap();
    }
    assert!(f.shortcut(session, "v").is_err());
    let effect = f.effect();
    let request = match effect {
        HostEffect::ReadClipboard { request } => request,
        _ => unreachable!(),
    };
    let (peer, _outputs) = f.app.create_ui(f.definition, settings(200.0, 1.0)).unwrap();
    assert!(
        f.app
            .ui_command(UiCommand::Clipboard(ClipboardReply {
                instance: peer,
                request,
                result: Ok(Some("foreign".into()))
            }))
            .is_err()
    );
    f.reply(effect, Ok(Some("correct".into()))).unwrap();
    assert_eq!(f.state(0).content, "correct");
    // Other replies captured the old content revision.
    for _ in 1..16 {
        f.reply(f.effect(), Ok(Some("stale".into()))).unwrap();
    }
    assert_eq!(f.state(0).content, "correct");
}

#[test]
fn caret_blink_deadlines_stop_on_selection_blur_and_hide() {
    let f = Fixture::new::<0>("abc", false);
    f.app
        .ui_command(UiCommand::Present {
            instance: f.instance,
            settings: PresentationSettings {
                timestamp_us: None,
                ..settings(400.0, 1.0)
            },
        })
        .unwrap();
    f.focus(0);
    let session = f.session();
    let output = f.outputs.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(output.redraw_after.is_some());
    assert!(!output.animating);
    f.shortcut(session, "a").unwrap();
    let output = f.outputs.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(output.redraw_after.is_none());
    f.named(session, "End", false);
    assert!(
        f.outputs
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .redraw_after
            .is_some()
    );
    f.send(Some(session), UiInput::Focused(false)).unwrap();
    assert!(
        f.outputs
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .redraw_after
            .is_none()
    );
    f.app
        .ui_command(UiCommand::Visibility {
            instance: f.instance,
            visible: false,
        })
        .unwrap();
    f.app.inspect(|_| Ok(())).unwrap();
    assert!(f.outputs.try_recv().is_err());
    assert!(f.text(Some(session), "hidden").is_err());
}

#[pixui_reflect::reflect]
mod rows {
    pub struct Item {
        pub id: u64,
        pub content: String,
    }
    pub struct Data {
        pub items: Vec<Item>,
        pub show: bool,
    }
}
#[pixui_engine::application::action::slice_actions(slice = "rows", facade = RowActions)]
mod row_actions {
    use super::*;
    #[action]
    pub fn reverse(mut data: EntityMut<rows::Data>) {
        data.items.reverse();
    }
    #[action]
    pub fn toggle(mut data: EntityMut<rows::Data>) {
        data.show = !data.show;
    }
}
fn row_data<'a>(context: &ExpressionContext<'a>) -> PixuiResult<&'a rows::Data> {
    let app = context.application()?;
    app.entity(app.slice_named("rows")?.id(), "data")
}

#[test]
fn keyed_reordering_preserves_selection_and_match_removal_discards_editing_state() {
    use pixui_engine::live_model::{
        identity::ItemKey,
        match_part::{MatchCandidate, MatchPart, MatchPattern},
        part::{ForLoopPart, LivePart},
    };
    let app = Application::new();
    let mut slice = ApplicationSlice::new("rows");
    slice
        .bind(
            "data",
            rows::Data {
                items: vec![
                    rows::Item {
                        id: 1,
                        content: "alpha".into(),
                    },
                    rows::Item {
                        id: 2,
                        content: "beta".into(),
                    },
                ],
                show: true,
            },
        )
        .unwrap();
    let slice = app.add_slice(slice).unwrap();
    row_actions::RowActions::register(&app, slice).unwrap();
    let actions = row_actions::RowActions::bind(&app).unwrap();
    let components = app.register_standard_components().unwrap();
    app.register_standard_painters().unwrap();
    let input = ComponentPart::typed(components.text_input, |context, _| {
        Ok(TextInputProps {
            content: context
                .value()?
                .downcast_ref::<rows::Item>()
                .unwrap()
                .content
                .clone(),
        })
    });
    let template = LivePart::Match(
        MatchPart::new(
            Expression::computed(|context| {
                Ok(pixui_reflect::DynamicObject::from_reflect(
                    row_data(context)?.show,
                ))
            }),
            vec![MatchCandidate {
                pattern: MatchPattern::value(true),
                part: LivePart::ForLoop(
                    ForLoopPart::new(
                        Expression::computed(|context| {
                            Ok(pixui_reflect::DynamicObject::from_ref(
                                &row_data(context)?.items,
                            ))
                        }),
                        input.into(),
                    )
                    .with_key(|context| {
                        Ok(ItemKey::Integer(
                            context.value()?.downcast_ref::<rows::Item>().unwrap().id,
                        ))
                    }),
                ),
            }],
        )
        .unwrap(),
    );
    let definition = app
        .register_ui(UiDefinition::new("rows", template))
        .unwrap();
    let (instance, outputs) = app.create_ui(definition, settings(400.0, 1.0)).unwrap();
    let output = outputs.recv_timeout(Duration::from_secs(3)).unwrap();
    let target = app
        .inspect(move |app| app.uis().instance(instance)?.component_id(0))
        .unwrap();
    let path = target.path().clone();
    app.ui_command(UiCommand::Focus {
        instance,
        target: Some(target.clone()),
    })
    .unwrap();
    let mut session = None;
    while let Ok(command) = outputs.window_commands().try_recv() {
        if let WindowCommand::SetTextInput(input) = command {
            session = input.session;
        }
    }
    let mut key = KeyboardEvent::named("shortcut");
    key.key = Key::Character("a".into());
    if cfg!(target_os = "macos") {
        key.modifiers.super_key = true;
    } else {
        key.modifiers.control = true;
    }
    app.ui_command(UiCommand::TextInput {
        instance,
        revision: output.revision,
        session,
        input: Box::new(UiInput::Keyboard(key)),
    })
    .unwrap();
    actions.reverse().unwrap();
    let expected = target.clone();
    let expected_path = path.clone();
    app.inspect(move |app| {
        let definition = app.uis().definition(definition)?;
        assert_eq!(definition.state().focus.as_ref(), Some(&expected));
        assert_eq!(
            definition.state().text_inputs[&expected_path]
                .selection
                .range(),
            0..5
        );
        assert_eq!(
            app.uis().instance(instance)?.layout().text_inputs[1].path,
            expected_path
        );
        Ok(())
    })
    .unwrap();
    actions.toggle().unwrap();
    app.inspect(move |app| {
        assert!(
            app.uis()
                .definition(definition)?
                .state()
                .text_inputs
                .is_empty()
        );
        assert!(app.uis().definition(definition)?.state().focus.is_none());
        Ok(())
    })
    .unwrap();
    assert!(
        app.ui_command(UiCommand::TextInput {
            instance,
            revision: output.revision,
            session,
            input: Box::new(UiInput::ImeCommit("removed".into()))
        })
        .is_err()
    );
    actions.toggle().unwrap();
    app.inspect(move |app| {
        assert_eq!(
            app.uis().definition(definition)?.state().text_inputs[&path]
                .selection
                .head,
            0
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn programmatic_focus_never_reuses_a_tokenless_native_handshake() {
    let f = Fixture::new::<0>("", false);
    let id = f.instance;
    let point = f
        .app
        .inspect(move |app| {
            let bounds = app.uis().instance(id)?.layout().text_inputs[0].bounds;
            Ok(Point {
                x: bounds.x + 8.0,
                y: bounds.y + 8.0,
            })
        })
        .unwrap();
    f.app
        .ui_command(UiCommand::Input {
            instance: id,
            revision: f.revision,
            input: UiInput::MouseButton {
                button: MouseButton::Left,
                state: ButtonState::Pressed,
                position: point,
                modifiers: Modifiers::default(),
            },
        })
        .unwrap();
    f.focus(1);
    assert!(f.text(None, "must not reach second field").is_err());
    let mut key = KeyboardEvent::named("F11");
    key.repeat = false;
    f.send(None, UiInput::Keyboard(key)).unwrap(); // Global diagnostics retain their geometry-independent policy.
    assert!(f.state(1).content.is_empty());
}

#[test]
fn queued_edits_use_one_session_without_waiting_for_presentation() {
    let f = Fixture::new::<0>("", false);
    f.focus(0);
    let session = f.session();
    let mut pending = Vec::new();
    for _ in 0..20 {
        pending.push(
            f.app
                .try_ui_command(UiCommand::TextInput {
                    instance: f.instance,
                    revision: f.revision,
                    session: Some(session),
                    input: Box::new(UiInput::ImeCommit("x".into())),
                })
                .unwrap(),
        );
    }
    for reply in pending {
        reply.wait().unwrap();
    }
    assert_eq!(f.state(0).content, "x".repeat(20));
}

#[test]
fn native_refocus_accepts_ordered_typing_before_metadata_is_consumed() {
    let f = Fixture::new::<0>("", false);
    f.focus(0);
    f.app
        .ui_command(UiCommand::Input {
            instance: f.instance,
            revision: f.revision,
            input: UiInput::Focused(true),
        })
        .unwrap();
    f.text(None, "refocused").unwrap();
    assert_eq!(f.state(0).content, "refocused");
}
