//! Observable contracts for worker-owned definitions, instances, and input.

use crossbeam_channel::{TryRecvError, TrySendError, bounded};
use pixui_base::{Arena, PixuiResult, pixui_error};
use pixui_engine::{
    application::{
        app::Application, application_handle::ApplicationHandle,
        application_slice::ApplicationSlice, collection::Collection,
    },
    expression::{context::ExpressionContext, expression::Expression},
    live_model::{
        part::{ComponentPart, ForLoopPart, LivePart},
        state::{GenericComponentState, PartState},
    },
    ui::{
        definition::UiDefinition,
        display_list::{DrawCommand, RenderOutput},
        geometry::{Point, Size},
        input::{UiCommand, UiInput},
        instance::UiInstanceId,
        mailbox::OutputReceiver,
        presentation::{PresentationSettings, Theme},
        widget::Widget,
    },
};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

#[pixui_reflect::reflect]
mod model {
    pub struct Item {
        pub title: String,
        pub done: bool,
    }
}
use model::Item;

#[pixui_engine::application::action::slice_actions(slice = "test", facade = Actions)]
mod actions {
    use super::*;
    #[action]
    pub fn add(items: &mut Arena<Item>, title: String) {
        items.insert(Item { title, done: false });
    }
    #[action]
    pub fn remove_first(items: &mut Arena<Item>) {
        let key = items.iter().next().map(|(key, _)| key);
        if let Some(key) = key {
            items.remove(key);
        }
    }
    #[action]
    pub fn mark(item: &mut Item) {
        item.done = true;
    }
    #[action]
    pub fn mutate_then_fail(items: &mut Arena<Item>) -> PixuiResult<()> {
        items.insert(Item {
            title: "changed despite error".into(),
            done: false,
        });
        Err(pixui_error!("deliberate action failure"))
    }
}

static NEXT_STATE: AtomicU64 = AtomicU64::new(1);

fn row(context: &ExpressionContext<'_>, _: &PresentationSettings) -> PixuiResult<Widget> {
    let app = context.application()?;
    let item = context.value()?.downcast_ref::<Item>().unwrap();
    if item.title == "render failure" {
        return Err(pixui_error!("deliberate render failure"));
    }
    let slice = app.slice_named("test")?;
    let arena = slice.collection("items")?.arena::<Item>().unwrap();
    let key = arena
        .iter()
        .find(|(_, candidate)| std::ptr::eq(*candidate, item))
        .unwrap()
        .0;
    let reference = app.object_ref(slice.id(), "items", key)?;
    let action = slice.action_handle_named("mark")?;
    Ok(Widget::Checkbox {
        text: item.title.clone(),
        checked: item.done,
        activate: Box::new(move |_| action.call(vec![Box::new(reference)])),
    })
}

fn setup(
    capacity: usize,
) -> (
    ApplicationHandle,
    actions::Actions,
    pixui_engine::ui::definition::UiDefinitionId,
) {
    let app = Application::with_capacity(capacity);
    let mut slice = ApplicationSlice::new("test");
    slice
        .add_collection(Collection::new_reflected::<Item>("items"))
        .unwrap();
    actions::Actions::register(&mut slice).unwrap();
    app.add_slice(slice).unwrap();
    let actions = actions::Actions::bind(&app).unwrap();
    let component = ComponentPart::new(|_| {
        Ok(GenericComponentState::new(
            NEXT_STATE.fetch_add(1, Ordering::Relaxed),
        ))
    })
    .with_presentation(row);
    let template = LivePart::ForLoop(ForLoopPart {
        expression: Expression::from_collection(app.collection_key("test", "items").unwrap()),
        body: Box::new(LivePart::Component(component)),
    });
    let definition = app
        .register_ui(UiDefinition::new("items", template))
        .unwrap();
    (app, actions, definition)
}

fn output(receiver: &OutputReceiver) -> RenderOutput {
    receiver.recv_timeout(Duration::from_secs(3)).unwrap()
}

fn state_ids(app: &ApplicationHandle, id: UiInstanceId) -> Vec<u64> {
    app.inspect(move |app| {
        let PartState::ForLoop(state) = app.uis().instance(id)?.state().root_state() else {
            panic!("loop");
        };
        Ok(state
            .items
            .iter()
            .map(|state| {
                let PartState::Component(state) = state else {
                    panic!("component");
                };
                *state.state.downcast_ref::<u64>().unwrap()
            })
            .collect())
    })
    .unwrap()
}

fn click(app: &ApplicationHandle, output: &RenderOutput) -> PixuiResult<()> {
    let id = output.instance_id;
    let point = app.inspect(move |app| {
        let bounds = app.uis().instance(id)?.layout().hit_regions[0].bounds;
        Ok(Point {
            x: bounds.x + 1.0,
            y: bounds.y + 1.0,
        })
    })?;
    app.ui_command(UiCommand::Input {
        instance: id,
        revision: output.revision,
        input: UiInput::Activate(point),
    })
}

#[test]
fn instances_have_independent_persistent_state_and_shared_data() {
    let (app, actions, definition) = setup(128);
    actions.add("one").unwrap();
    let (first, a) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let (second, b) = app
        .create_ui(
            definition,
            PresentationSettings {
                theme: Theme::Dark,
                viewport: Size {
                    width: 200.0,
                    height: 100.0,
                },
                ..Default::default()
            },
        )
        .unwrap();
    let before_a = output(&a);
    let before_b = output(&b);
    assert_ne!(before_a.display_list, before_b.display_list);
    let ids_a = state_ids(&app, first);
    let ids_b = state_ids(&app, second);
    assert_ne!(ids_a, ids_b);
    actions.add("two").unwrap();
    output(&a);
    output(&b);
    assert_eq!(state_ids(&app, first)[0], ids_a[0]);
    assert_eq!(state_ids(&app, second)[0], ids_b[0]);
    assert_eq!(state_ids(&app, first).len(), 2);
    actions.remove_first().unwrap();
    output(&a);
    output(&b);
    assert_eq!(state_ids(&app, first), ids_a); // Explicit positional reconciliation.
    assert_eq!(state_ids(&app, second), ids_b);
    assert!(click(&app, &before_a).is_err());
}

#[test]
fn current_click_invokes_action_but_stale_deleted_and_closed_targets_fail() {
    let (app, actions, definition) = setup(128);
    actions.add("one").unwrap();
    let (id, receiver) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let initial = output(&receiver);
    click(&app, &initial).unwrap();
    let marked = output(&receiver);
    assert!(
        app.inspect(|app| Ok(app
            .slice_named("test")?
            .collection("items")?
            .arena::<Item>()
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .1
            .done))
            .unwrap()
    );
    assert!(click(&app, &initial).is_err());
    app.ui_command(UiCommand::Input {
        instance: id,
        revision: initial.revision,
        input: UiInput::PointerMoved(Point { x: 20.0, y: 20.0 }),
    })
    .unwrap();
    actions.remove_first().unwrap();
    output(&receiver);
    assert!(
        app.ui_command(UiCommand::Input {
            instance: id,
            revision: marked.revision,
            input: UiInput::Activate(Point { x: 20.0, y: 20.0 })
        })
        .is_err()
    );
    app.ui_command(UiCommand::Close { instance: id }).unwrap();
    assert!(app.ui_command(UiCommand::Close { instance: id }).is_err());
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn settings_updates_and_latest_output_replacement_do_not_block_worker() {
    let (app, actions, definition) = setup(128);
    let (id, receiver) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let initial = output(&receiver);
    for index in 0..20 {
        actions.add(format!("item {index}")).unwrap();
    }
    app.ui_command(UiCommand::Present {
        instance: id,
        settings: PresentationSettings {
            theme: Theme::Dark,
            viewport: Size {
                width: 180.0,
                height: 80.0,
            },
            scale_factor: 2.0,
            ..Default::default()
        },
    })
    .unwrap();
    // Wait for the final revision via inspection without consuming any frames.
    let expected = app
        .inspect(move |app| Ok(app.uis().instance(id)?.revision()))
        .unwrap();
    let mut latest = output(&receiver);
    if latest.revision < expected {
        latest = output(&receiver);
    }
    assert!(latest.revision > initial.revision);
    assert_eq!(latest.revision, expected);
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
    let state = app
        .inspect(move |app| {
            Ok((
                app.uis().instance(id)?.settings().scale_factor,
                app.uis().instance(id)?.layout().hit_regions.len(),
            ))
        })
        .unwrap();
    assert_eq!(state, (2.0, 20));
    let invalid = PresentationSettings {
        scale_factor: f32::NAN,
        ..Default::default()
    };
    assert!(
        app.ui_command(UiCommand::Present {
            instance: id,
            settings: invalid
        })
        .is_err()
    );
    assert_eq!(state_ids(&app, id).len(), 20);
    drop(receiver);
    // Cleanup runs after each command batch; a following barrier sees removal.
    app.inspect(|_| Ok(())).unwrap();
    assert!(
        app.inspect(move |app| Ok(app.uis().instance(id).is_err()))
            .unwrap()
    );
}

#[test]
fn failed_actions_invalidate_and_failed_renders_retain_last_good_geometry() {
    let (app, actions, definition) = setup(128);
    let (id, receiver) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    output(&receiver);
    assert!(actions.mutate_then_fail().is_err());
    let good = output(&receiver);
    assert_eq!(state_ids(&app, id).len(), 1);
    actions.add("render failure").unwrap();
    // Rendering completes at batch boundaries; poll the inspectable error.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let failed = app
            .inspect(move |app| Ok(app.uis().instance(id)?.last_error().is_some()))
            .unwrap();
        if failed {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(
        app.inspect(move |app| Ok(app.uis().instance(id)?.revision()))
            .unwrap(),
        good.revision
    );
    assert!(click(&app, &good).is_err()); // Data changed; old geometry cannot retarget.
}

#[test]
fn nonblocking_input_preserves_commands_when_the_worker_queue_is_full() {
    let (app, _, definition) = setup(1);
    let (id, receiver) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    output(&receiver);
    let (entered, enter) = bounded(1);
    let (release, wait) = bounded(1);
    let blocked = app.clone();
    let thread = std::thread::spawn(move || {
        blocked.inspect(move |_| {
            entered.send(()).unwrap();
            wait.recv().unwrap();
            Ok(())
        })
    });
    enter.recv_timeout(Duration::from_secs(2)).unwrap();
    let first = app
        .try_ui_command(UiCommand::Present {
            instance: id,
            settings: PresentationSettings::default(),
        })
        .unwrap();
    let command = match app.try_ui_command(UiCommand::Close { instance: id }) {
        Err(TrySendError::Full(command)) => command,
        _ => panic!("full queue must return the command"),
    };
    assert!(matches!(command, UiCommand::Close { instance } if instance == id));
    release.send(()).unwrap();
    thread.join().unwrap().unwrap();
    first.wait().unwrap();
    app.ui_command(command).unwrap();
    assert!(
        app.inspect(move |app| Ok(app.uis().instance(id).is_err()))
            .unwrap()
    );
}

#[test]
fn scrolling_focus_and_boundary_hit_tests_use_instance_layout() {
    let (app, actions, definition) = setup(128);
    for _ in 0..5 {
        actions.add("long title wraps in small windows").unwrap();
    }
    let (id, receiver) = app
        .create_ui(
            definition,
            PresentationSettings {
                viewport: Size {
                    width: 180.0,
                    height: 100.0,
                },
                ..Default::default()
            },
        )
        .unwrap();
    let initial = output(&receiver);
    app.ui_command(UiCommand::Input {
        instance: id,
        revision: initial.revision,
        input: UiInput::FocusNext,
    })
    .unwrap();
    let focused = output(&receiver);
    app.ui_command(UiCommand::Input {
        instance: id,
        revision: focused.revision,
        input: UiInput::ActivateFocused,
    })
    .unwrap();
    let marked = output(&receiver);
    app.ui_command(UiCommand::Input {
        instance: id,
        revision: marked.revision,
        input: UiInput::Scroll(10000.0),
    })
    .unwrap();
    output(&receiver);
    assert!(
        app.inspect(move |app| Ok(app.uis().instance(id)?.scroll_offset() > 0.0))
            .unwrap()
    );
    let bounds = pixui_engine::ui::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    assert!(bounds.contains(Point { x: 0.0, y: 0.0 }));
    assert!(!bounds.contains(Point { x: 10.0, y: 5.0 }));
}

#[test]
fn definition_registration_and_owned_sendable_boundary_are_explicit() {
    fn assert_send<T: Send + 'static>() {}
    assert_send::<UiDefinition>();
    assert_send::<RenderOutput>();
    assert_send::<UiCommand>();
    let app = Application::new();
    let template = LivePart::Component(ComponentPart::default());
    assert!(
        app.register_ui(UiDefinition::new("", template.clone()))
            .is_err()
    );
    let first = app
        .register_ui(UiDefinition::new("first", template.clone()))
        .unwrap();
    assert!(
        app.register_ui(UiDefinition::new("first", template.clone()))
            .is_err()
    );
    let second = app
        .register_ui(UiDefinition::new("second", template))
        .unwrap();
    assert_ne!(first, second);
    let (id, receiver) = app
        .create_ui(first, PresentationSettings::default())
        .unwrap();
    assert!(
        !output(&receiver)
            .display_list
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::DrawText { .. }))
    );
    app.ui_command(UiCommand::Close { instance: id }).unwrap();
}
