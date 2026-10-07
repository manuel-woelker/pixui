//! The todo title reflects open tasks, independently of filtering and animation.
use pixui_engine::{
    application::app::Application,
    ui::{presentation::PresentationSettings, window_properties::WindowCommand},
};
use pixui_example_todo::{
    gui_ui, orbiting_comets,
    todo::{self, actions::TodoActions},
};
use std::time::Duration;
#[test]
fn title_counts_open_todos_and_logo_snapshot_is_shared_with_icon() {
    let app = Application::new();
    todo::create_slice(&app).unwrap();
    let actions = TodoActions::bind(&app).unwrap();
    let first = actions.add_todo("First").unwrap();
    actions.add_todo("Second").unwrap();
    let components = app.register_standard_components().unwrap();
    app.register_standard_painters().unwrap();
    let comets = orbiting_comets::register(&app).unwrap();
    let definition = app
        .register_ui(gui_ui::definition(&app, components, comets).unwrap())
        .unwrap();
    let (_, outputs) = app
        .create_ui(
            definition,
            PresentationSettings {
                timestamp_us: Some(0),
                ..Default::default()
            },
        )
        .unwrap();
    let frame = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
    app.inspect(|_| Ok(())).unwrap();
    let commands = outputs.window_commands();
    assert_eq!(
        commands.try_recv().unwrap(),
        WindowCommand::SetTitle("Todos — 2 open".into())
    );
    assert_eq!(
        commands.try_recv().unwrap(),
        WindowCommand::SetIcon(Some(frame.display_list.images[1].clone()))
    );
    actions.add_todo("Third").unwrap();
    app.inspect(|_| Ok(())).unwrap();
    assert_eq!(
        commands.try_recv().unwrap(),
        WindowCommand::SetTitle("Todos — 3 open".into())
    );
    assert!(
        commands.try_recv().is_err(),
        "unchanged icon must not be resent"
    );
    let reference = app.object_ref(actions.slice_id(), "todos", first).unwrap();
    actions.mark_done(reference).unwrap();
    app.inspect(|_| Ok(())).unwrap();
    assert_eq!(
        commands.try_recv().unwrap(),
        WindowCommand::SetTitle("Todos — 2 open".into())
    );
    actions.toggle_hide_completed().unwrap();
    actions.toggle_animation().unwrap();
    app.inspect(|_| Ok(())).unwrap();
    assert!(
        commands.try_recv().is_err(),
        "unrelated changes must not send metadata"
    );
}
