//! Exercise real worker rendering and input bindings without opening native windows.
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    ui::{
        display_list::{DrawCommand, RenderOutput},
        geometry::{Point, Size},
        input::{ButtonState, KeyboardEvent, MouseButton, UiCommand, UiInput},
        instance::UiInstanceId,
        mailbox::OutputReceiver,
        presentation::{PresentationSettings, Theme},
        window_properties::WindowCommand,
    },
};
use pixui_example_showcase::{model::actions::ShowcaseActions, setup};
use std::time::Duration;

fn receive(outputs: &OutputReceiver) -> RenderOutput {
    outputs.recv_timeout(Duration::from_secs(2)).unwrap()
}

fn has_text(output: &RenderOutput, expected: &str) -> bool {
    output
        .display_list
        .commands
        .iter()
        .any(|command| matches!(command, DrawCommand::DrawText { text, .. } if text == expected))
}

fn click(app: &ApplicationHandle, instance: UiInstanceId, output: &RenderOutput, index: usize) {
    let position = app
        .inspect(move |app| {
            let rect = app.uis().instance(instance)?.layout().hit_regions[index].bounds;
            Ok(Point {
                x: rect.x + 4.0,
                y: rect.y + 4.0,
            })
        })
        .unwrap();
    app.ui_command(UiCommand::Input {
        instance,
        revision: output.revision,
        input: UiInput::MouseButton {
            button: MouseButton::Left,
            state: ButtonState::Released,
            position,
            modifiers: Default::default(),
        },
    })
    .unwrap();
}

#[test]
fn sidebar_is_fixed_and_footer_follows_window_height() {
    let (app, definition) = setup::create().unwrap();
    for (width, height, language) in [(800.0, 600.0, "en"), (1100.0, 800.0, "de")] {
        let settings = app
            .presentation_language(
                PresentationSettings {
                    viewport: Size { width, height },
                    ..Default::default()
                },
                language,
            )
            .unwrap();
        let (instance, outputs) = app.create_ui(definition, settings).unwrap();
        receive(&outputs);
        app.inspect(move |app| {
            let boxes = &app.uis().instance(instance)?.layout().component_bounds;
            for button in &boxes[2..7] {
                assert_eq!(
                    button.width, 200.0,
                    "navigation buttons fill the fixed sidebar"
                );
            }
            assert_eq!(boxes[7].x, 236.0, "detail pane follows sidebar and gap");
            let footer = boxes.last().unwrap();
            assert_eq!(footer.y + footer.height, height - 16.0);
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn widgets_update_shared_windows_conditional_content_collection_and_title() {
    let (app, definition) = setup::create().unwrap();
    let (one, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let (_, other) = app
        .create_ui(
            definition,
            app.presentation_language(
                PresentationSettings {
                    theme: Theme::Dark,
                    ..Default::default()
                },
                "de",
            )
            .unwrap(),
        )
        .unwrap();
    let mut current = receive(&outputs);
    let german = receive(&other);
    assert!(has_text(&current, "Counter: 0"));
    assert!(has_text(&german, "Zähler: 0"));
    assert!(has_text(&current, "Buttons"));
    assert!(!has_text(&current, "Sample 1"));
    assert_ne!(current.display_list, german.display_list);
    assert!(!current.animating);
    assert_eq!(current.redraw_after, None);
    assert!(current.display_list.images.is_empty());
    app.inspect(move |app| {
        let boxes = &app.uis().instance(one)?.layout().component_bounds;
        assert!(boxes[0].width > boxes[1].width, "title spans both panes");
        assert!(boxes[7].x >= boxes[1].x + boxes[1].width + 19.0);
        assert_eq!(boxes[7].y, boxes[1].y, "pane headings align");
        Ok(())
    })
    .unwrap();
    assert!(!current.display_list.fonts.is_empty());
    current.display_list.validate().unwrap();
    assert!(matches!(outputs.window_commands().try_recv().unwrap(),
        WindowCommand::SetTitle(title) if title.as_str() == "PixUI Showcase — 0"));
    assert!(matches!(
        outputs.window_commands().try_recv().unwrap(),
        WindowCommand::SetIcon(Some(_))
    ));

    click(&app, one, &current, 5);
    current = receive(&outputs);
    assert!(has_text(&current, "Counter: 1"));
    assert!(has_text(&receive(&other), "Zähler: 1"));
    assert!(matches!(outputs.window_commands().try_recv().unwrap(),
        WindowCommand::SetTitle(title) if title.as_str() == "PixUI Showcase — 1"));

    click(&app, one, &current, 1);
    current = receive(&outputs);
    let german_checkboxes = receive(&other);
    assert!(has_text(&current, "Checkboxes"));
    assert!(has_text(&german_checkboxes, "Kontrollkästchen"));
    assert!(!has_text(&current, "Counter: 1"));
    click(&app, one, &current, 5);
    current = receive(&outputs);
    receive(&other);
    app.inspect(|app| {
        let slice = app.slice_named("showcase")?.id();
        assert!(*app.entity::<bool>(slice, "checked")?);
        assert!(*app.entity::<bool>(slice, "details")?);
        Ok(())
    })
    .unwrap();

    let explanation = "Shared data, hover and focus · independent theme and locale";
    assert!(has_text(&current, explanation));
    let initial_bounds = app
        .inspect(move |app| Ok(app.uis().instance(one)?.layout().component_bounds.len()))
        .unwrap();
    click(&app, one, &current, 6);
    current = receive(&outputs);
    receive(&other);
    assert!(!has_text(&current, explanation));
    app.inspect(move |app| {
        assert_eq!(
            app.uis().instance(one)?.layout().component_bounds.len(),
            initial_bounds - 1
        );
        Ok(())
    })
    .unwrap();

    click(&app, one, &current, 2);
    current = receive(&outputs);
    let german_images = receive(&other);
    assert_eq!(current.display_list.images.len(), 1);
    assert_eq!(
        current.display_list.images[0],
        german_images.display_list.images[0]
    );
    assert!(!has_text(&current, explanation));
    click(&app, one, &current, 3);
    current = receive(&outputs);
    receive(&other);
    assert!(has_text(&current, "Sample 1"));
    click(&app, one, &current, 5);
    current = receive(&outputs);
    assert!(has_text(&current, "Sample 2"));
    assert!(has_text(&receive(&other), "Sample 2"));

    click(&app, one, &current, 0);
    current = receive(&outputs);
    receive(&other);
    assert!(has_text(&current, "Counter: 1"));
    click(&app, one, &current, 6);
    current = receive(&outputs);
    assert!(has_text(&current, "Counter: 0"));
    receive(&other);

    click(&app, one, &current, 1);
    current = receive(&outputs);
    receive(&other);
    assert!(
        !has_text(&current, explanation),
        "page switching retains demo data"
    );
    click(&app, one, &current, 6);
    current = receive(&outputs);
    assert!(has_text(&current, explanation));
    receive(&other);
    click(&app, one, &current, 4);
    current = receive(&outputs);
    let german_text = receive(&other);
    assert!(has_text(&current, "Text and translations"));
    assert!(has_text(&german_text, "Text und Übersetzungen"));
    assert!(has_text(&current, "Counter: 0"));
    assert!(!has_text(&current, "Show details"));
}

#[test]
fn keyboard_navigation_activates_buttons_and_facade_updates_named_entities() {
    let (app, definition) = setup::create().unwrap();
    let (instance, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let initial = receive(&outputs);
    app.ui_command(UiCommand::Input {
        instance,
        revision: initial.revision,
        input: UiInput::Keyboard(KeyboardEvent::named("Tab")),
    })
    .unwrap();
    let focused = receive(&outputs);
    app.ui_command(UiCommand::Input {
        instance,
        revision: focused.revision,
        input: UiInput::Keyboard(KeyboardEvent::named("Enter")),
    })
    .unwrap();
    let selected = receive(&outputs);
    assert!(has_text(&selected, "Buttons"));
    let mut focused = selected;
    for _ in 0..2 {
        app.ui_command(UiCommand::Input {
            instance,
            revision: focused.revision,
            input: UiInput::Keyboard(KeyboardEvent::named("Tab")),
        })
        .unwrap();
        focused = receive(&outputs);
    }
    app.ui_command(UiCommand::Input {
        instance,
        revision: focused.revision,
        input: UiInput::Keyboard(KeyboardEvent::named("Enter")),
    })
    .unwrap();
    let checkbox_page = receive(&outputs);
    assert!(has_text(&checkbox_page, "Checkboxes"));
    assert!(!has_text(&checkbox_page, "Counter: 0"));
    let actions = ShowcaseActions::bind(&app).unwrap();
    actions.increment().unwrap();
    receive(&outputs);
    actions.select(0).unwrap();
    assert!(has_text(&receive(&outputs), "Counter: 1"));
    actions.reset().unwrap();
    assert!(has_text(&receive(&outputs), "Counter: 0"));
    actions.toggle_checked().unwrap();
    receive(&outputs);
    actions.toggle_checked().unwrap();
    receive(&outputs);
    app.inspect(|app| {
        let slice = app.slice_named("showcase")?.id();
        assert!(!*app.entity::<bool>(slice, "checked")?);
        assert_eq!(*app.entity::<u64>(slice, "selected")?, 0);
        Ok(())
    })
    .unwrap();
    assert!(actions.select(5).is_err());
}

#[test]
fn translation_export_runs_without_a_display_and_is_reproducible() {
    let path = std::env::temp_dir().join(format!("pixui-showcase-{}.pot", std::process::id()));
    let run = |flags: &[&str]| {
        let status = std::process::Command::new(env!("CARGO_BIN_EXE_pixui-example-showcase"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .args(flags)
            .args(["--export-translations", path.to_str().unwrap()])
            .status()
            .unwrap();
        assert!(status.success());
        std::fs::read_to_string(&path).unwrap()
    };
    let first = run(&[]);
    assert_eq!(first, run(&["--no-hot-reload"]));
    assert_eq!(first, run(&["--hot-reload"]));
    assert!(first.contains("Counter: {count}"));
    assert!(first.contains("Show details"));
    assert!(first.contains("PixUI Showcase — {count}"));
    std::fs::remove_file(path).unwrap();
}
