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
        .any(|command| matches!(command, DrawCommand::DrawText { text, .. } | DrawCommand::StyledText { text, .. } if text == expected))
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
            state: ButtonState::Pressed,
            position,
            modifiers: Default::default(),
        },
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
    app.inspect(|_| Ok(())).unwrap();
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
            for button in &boxes[2..10] {
                assert_eq!(
                    button.width, 200.0,
                    "navigation buttons fill the fixed sidebar"
                );
            }
            assert_eq!(boxes[10].x, 236.0, "detail pane follows sidebar and gap");
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
        assert!(boxes[10].x >= boxes[1].x + boxes[1].width + 19.0);
        assert_eq!(boxes[10].y, boxes[1].y, "pane headings align");
        Ok(())
    })
    .unwrap();
    assert!(!current.display_list.fonts.is_empty());
    pixui_engine::ui::render_input::RenderInput::from(&current)
        .validate()
        .unwrap();
    assert!(matches!(outputs.window_commands().try_recv().unwrap(),
        WindowCommand::SetTitle(title) if title.as_str() == "PixUI Showcase — 0"));
    assert!(matches!(
        outputs.window_commands().try_recv().unwrap(),
        WindowCommand::SetIcon(Some(_))
    ));

    click(&app, one, &current, 8);
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
    click(&app, one, &current, 8);
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
    click(&app, one, &current, 9);
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
    for output in [&current, &german_images] {
        assert!(output.display_list.commands.iter().any(|command| matches!(
            command, DrawCommand::DrawImage { destination, .. }
                if destination.width == 160.0 && destination.height == 160.0
        )));
    }
    assert_eq!(
        current.display_list.images[0],
        german_images.display_list.images[0]
    );
    assert!(!has_text(&current, explanation));
    click(&app, one, &current, 3);
    current = receive(&outputs);
    receive(&other);
    assert!(has_text(&current, "Sample 1"));
    click(&app, one, &current, 8);
    current = receive(&outputs);
    assert!(has_text(&current, "Sample 2"));
    assert!(has_text(&receive(&other), "Sample 2"));

    click(&app, one, &current, 0);
    current = receive(&outputs);
    receive(&other);
    assert!(has_text(&current, "Counter: 1"));
    click(&app, one, &current, 9);
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
    click(&app, one, &current, 9);
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
    for _ in 0..1 {
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
        assert_eq!(
            app.entity::<pixui_example_showcase::model::data::NavigationState>(
                slice,
                "navigation"
            )?
            .selected,
            0
        );
        Ok(())
    })
    .unwrap();
    assert!(actions.select(8).is_err());
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
    assert!(first.contains("msgid \"Images\""));
    assert!(!first.contains("pixui-logo.png"));
    assert!(first.contains("Counter: {entities_count}"));
    assert!(!first.contains("Counter: {count}"));
    assert!(first.contains("Show details"));
    assert!(first.contains("PixUI Showcase — {count}"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn text_page_has_a_focus_target_without_an_activation() {
    let (app, definition) = setup::create().unwrap();
    let (instance, outputs) = app.create_ui(definition, Default::default()).unwrap();
    receive(&outputs);
    ShowcaseActions::bind(&app).unwrap().select(4).unwrap();
    let text_page = receive(&outputs);
    assert!(has_text(&text_page, "Focus without activation"));
    let target = app
        .inspect(move |app| {
            let instance = app.uis().instance(instance)?;
            let target = instance.layout().focus_targets.last().unwrap();
            instance.component_id(target.component_index)
        })
        .unwrap();
    app.ui_command(UiCommand::Focus {
        instance,
        target: Some(target.clone()),
    })
    .unwrap();
    let focused = receive(&outputs);
    app.ui_command(UiCommand::Input {
        instance,
        revision: focused.revision,
        input: UiInput::Keyboard(KeyboardEvent::named("Enter")),
    })
    .unwrap();
    app.inspect(move |app| {
        assert_eq!(
            app.uis().definition(definition)?.state().focus,
            Some(target)
        );
        Ok(())
    })
    .unwrap();
    assert!(outputs.try_recv().is_err());
}

#[test]
fn style_gallery_resolves_variants_local_overrides_and_per_window_fonts() {
    use pixui_engine::ui::render_input::{RenderInput, ResolvedCommand};
    let (app, definition) = setup::create().unwrap();
    let (_, light) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let (_, dark) = app
        .create_ui(
            definition,
            PresentationSettings {
                theme: Theme::Dark,
                ..Default::default()
            },
        )
        .unwrap();
    receive(&light);
    receive(&dark);
    let actions = ShowcaseActions::bind(&app).unwrap();
    actions.select(5).unwrap();
    let first = receive(&light);
    let second = receive(&dark);
    for output in [&first, &second] {
        for title in [
            "Styles",
            "Primary",
            "Quiet",
            "Danger",
            "Compact",
            "Monospace",
            "Local purple override",
            "Dynamic style variant",
        ] {
            assert!(has_text(output, title));
        }
        let input = RenderInput::from(output);
        input.validate().unwrap();
        assert!(output.display_list.commands.iter().any(|command| matches!(
            input.resolve(command).unwrap(),
            ResolvedCommand::FillRect {
                color: pixui_engine::ui::display_list::Color(100, 45, 150),
                ..
            }
        )));
    }
    assert_ne!(first.styles, second.styles);
    assert!(
        first.display_list.fonts.len() > 1,
        "regular plus explicit monospace variant"
    );
    let before = app.inspect(move |app| Ok(app.styles().revision())).unwrap();
    actions.toggle_checked().unwrap();
    let compact = receive(&light);
    receive(&dark);
    let dynamic_index = |output: &RenderOutput| {
        output
            .display_list
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::StyledText { text, style, .. } if text == "Dynamic style variant" => {
                    Some(*style)
                }
                _ => None,
            })
            .unwrap()
    };
    assert_ne!(
        dynamic_index(&compact),
        dynamic_index(&first),
        "the compact recipe is already in the table; this occurrence switches its command index"
    );
    assert_eq!(
        before,
        app.inspect(|app| Ok(app.styles().revision())).unwrap(),
        "local dynamic variants leave the registry unchanged"
    );
    RenderInput::from(&first).validate().unwrap();
}

#[test]
fn layout_gallery_places_nested_cards_wraps_actions_and_dispatches_clicks() {
    let (app, definition) = setup::create().unwrap();
    ShowcaseActions::bind(&app).unwrap().select(6).unwrap();
    for width in [800.0, 1200.0] {
        let (instance, outputs) = app
            .create_ui(
                definition,
                PresentationSettings {
                    viewport: Size {
                        width,
                        height: 1000.0,
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        app.inspect(move |app| {
            assert_eq!(app.uis().instance(instance)?.last_error(), None);
            Ok(())
        })
        .unwrap();
        let output = outputs
            .recv_timeout(Duration::from_secs(2))
            .expect("initial layout gallery output");
        assert!(has_text(&output, "Nested Grid and Flex"));
        assert!(has_text(&output, "Activity"));
        assert!(has_text(&output, "Archive project"));
        assert!(has_text(&output, "Project 36"));
        let task_rows = output.display_list.commands.iter().filter(|command| matches!(
            command, DrawCommand::DrawText { text, .. } | DrawCommand::StyledText { text, .. } if text == "Task 06"
        )).count();
        assert_eq!(
            task_rows, 36,
            "each live project loop expands its own task loop"
        );
        pixui_engine::ui::render_input::RenderInput::from(&output)
            .validate()
            .unwrap();
        app.inspect(move |app| {
            let boxes = &app.uis().instance(instance)?.layout().component_bounds;
            assert!(
                boxes.len() >= 600,
                "layout stress pane prepares all loop occurrences"
            );
            // Six metric cards form two rows of three inside the spanning main pane.
            assert!(boxes[16].x < boxes[18].x && boxes[18].x < boxes[20].x);
            assert_eq!(boxes[16].y, boxes[20].y);
            assert!(boxes[22].y > boxes[16].y);
            assert!(
                boxes[29].x > boxes[20].x,
                "activity occupies the third outer track"
            );
            let first_action = boxes[34];
            let last_action = boxes[39];
            if width == 800.0 {
                assert!(last_action.y > first_action.y, "narrow action row wraps");
            } else {
                assert_eq!(
                    last_action.y, first_action.y,
                    "wide action row fits on one line"
                );
            }
            Ok(())
        })
        .unwrap();
        // Eight navigation buttons and two toolbar buttons precede the action row.
        click(&app, instance, &output, 10);
        let changed = outputs
            .recv_timeout(Duration::from_secs(2))
            .expect("layout action output");
        assert!(has_text(
            &changed,
            if width == 800.0 {
                "Counter: 1"
            } else {
                "Counter: 2"
            }
        ));
    }
}

#[test]
fn pixui_pane_renders_translates_and_dispatches_compiled_actions() {
    let (app, definition) = setup::create().unwrap();
    let actions = ShowcaseActions::bind(&app).unwrap();
    actions.select(7).unwrap();
    let (instance, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let (_, german) = app
        .create_ui(
            definition,
            app.presentation_language(PresentationSettings::default(), "de")
                .unwrap(),
        )
        .unwrap();
    let initial = receive(&outputs);
    assert!(has_text(&initial, "Defined in a .pixui file"));
    assert!(has_text(&initial, "Untranslated template: 0"));
    let translated = receive(&german);
    assert!(has_text(&translated, "In einer .pixui Datei definiert"));
    assert!(has_text(&translated, "Untranslated template: 0"));
    click(&app, instance, &initial, 8);
    let incremented = receive(&outputs);
    assert!(has_text(&incremented, "Counter: 1"));
    assert!(has_text(&incremented, "Untranslated template: 1"));
    assert!(has_text(&receive(&german), "Zähler: 1"));
    click(&app, instance, &incremented, 9);
    let appended = receive(&outputs);
    assert!(has_text(&appended, "Sample 2"));
    receive(&german);
    click(&app, instance, &appended, 10);
    assert!(has_text(&receive(&outputs), "Details hidden"));
    assert!(has_text(&receive(&german), "Details ausgeblendet"));
}

#[test]
fn navigation_list_accepts_new_component_refs_without_changing_the_shell() {
    use pixui_engine::{
        components::label::LabelComponent,
        ui_language::{compiler::compile, resolution::environment::Environment},
    };
    use pixui_example_showcase::model::data::{NavigationEntry, NavigationState};
    let (app, definition) = setup::create().unwrap();
    let (one, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let (_, german) = app
        .create_ui(
            definition,
            PresentationSettings {
                language: app
                    .inspect(|app| app.translations().language("de"))
                    .unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
    receive(&outputs);
    receive(&german);
    let extra = app
        .inspect(|app| {
            let mut env = Environment::new();
            env.component("Label", app.components().component::<LabelComponent>()?)?;
            Ok(compile(
                "fixture.pixui",
                "ui Extra {<Label text=\"Extra view\" />}",
                &env,
            )
            .unwrap()
            .into_template())
        })
        .unwrap();
    let component = app.register_template("fixture", "showcase", extra).unwrap();
    let title = app.localized_text("showcase", "Collections").unwrap();
    let mut entries = app
        .inspect(|app| {
            let slice = app.slice_named("showcase")?.id();
            Ok(app
                .entity::<NavigationState>(slice, "navigation")?
                .entries
                .clone())
        })
        .unwrap();
    entries.push(NavigationEntry {
        id: 99,
        title,
        component,
    });
    let actions = ShowcaseActions::bind(&app).unwrap();
    actions.install_navigation(entries).unwrap();
    let changed = receive(&outputs);
    let translated = receive(&german);
    assert!(has_text(&translated, "Sammlungen"));
    // The ninth navigation button is generated from the appended data entry.
    click(&app, one, &changed, 8);
    assert!(has_text(&receive(&outputs), "Extra view"));
    assert!(has_text(&receive(&german), "Extra view"));
    assert!(actions.select(999).is_err());
    app.inspect(|app| {
        let slice = app.slice_named("showcase")?.id();
        assert_eq!(
            app.entity::<NavigationState>(slice, "navigation")?.selected,
            99
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn failed_dynamic_components_keep_published_frames_and_recover() {
    use pixui_engine::{
        application::app::Application,
        expression::{context::ExpressionContext, expression::Expression},
        live_model::part::{CompositePart, DynamicPart, LivePart},
    };
    use pixui_example_showcase::model::data::NavigationState;
    fn selected(
        context: &ExpressionContext<'_>,
    ) -> pixui_base::PixuiResult<pixui_reflect::DynamicObject<'static>> {
        let app = context.application()?;
        let state =
            app.entity::<NavigationState>(app.slice_named("showcase")?.id(), "navigation")?;
        Ok(pixui_reflect::DynamicObject::from_reflect(
            state
                .entries
                .iter()
                .find(|entry| entry.id == state.selected)
                .unwrap()
                .component,
        ))
    }
    let (app, definition) = setup::create().unwrap();
    let (instance, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let mut successful = receive(&outputs);
    let actions = ShowcaseActions::bind(&app).unwrap();
    let mut entries = app
        .inspect(|app| {
            Ok(app
                .entity::<NavigationState>(app.slice_named("showcase")?.id(), "navigation")?
                .entries
                .clone())
        })
        .unwrap();
    let original = entries[0].component;
    let foreign = Application::default()
        .register_template(
            "foreign",
            "showcase",
            LivePart::Composite(CompositePart { parts: vec![] }),
        )
        .unwrap();
    let recursive = app
        .register_template(
            "recursive",
            "showcase",
            LivePart::Dynamic(DynamicPart::new(Expression::computed(selected))),
        )
        .unwrap();
    for (reference, message) in [(foreign, "foreign"), (recursive, "recursive")] {
        entries[0].component = reference;
        actions.install_navigation(entries.clone()).unwrap();
        let revision = successful.revision;
        app.inspect(move |app| {
            let instance = app.uis().instance(instance)?;
            assert_eq!(instance.revision(), revision);
            let error = instance.last_error().unwrap();
            assert!(error.contains(message), "{error}");
            assert!(error.contains("showcase.pixui"), "{error}");
            Ok(())
        })
        .unwrap();
        assert!(outputs.try_recv().is_err());
        entries[0].component = original;
        actions.install_navigation(entries.clone()).unwrap();
        successful = receive(&outputs);
        assert!(successful.revision > revision);
        assert!(has_text(&successful, "Counter: 0"));
    }
}
