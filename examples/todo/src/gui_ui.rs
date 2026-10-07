//! Typed component props and action bindings for the native todo example.

use crate::{
    orbiting_comets::{self, OrbitingComets},
    todo::TodoItem,
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    component_registry::component_id::ComponentId,
    components::{
        button::ButtonProps, checkbox::CheckboxProps, image::ImageProps, label::LabelProps,
    },
    expression::{context::ExpressionContext, expression::Expression},
    live_model::{
        match_part::{MatchCandidate, MatchPart, MatchPattern},
        part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
    },
    painters::standard::StandardComponents,
    ui::{activation::ActionBinding, definition::UiDefinition, presentation::PresentationSettings},
};
use pixui_reflect::Reflect;

pub fn definition(
    application: &ApplicationHandle,
    components: StandardComponents,
    comets: ComponentId<OrbitingComets>,
) -> PixuiResult<UiDefinition> {
    definition_with_assets(application, components, comets, None)
}

/// Creates a definition with an optional higher-priority resource directory.
pub fn definition_with_assets(
    application: &ApplicationHandle,
    components: StandardComponents,
    comets: ComponentId<OrbitingComets>,
    override_root: Option<&std::path::Path>,
) -> PixuiResult<UiDefinition> {
    crate::logo::configure_resources(application, override_root)?;
    let logo = ComponentPart::typed(components.image, |_, _| {
        ImageProps::new("images/pixui-logo.png")
    });
    let todos = application.collection_key("todo", "todos")?;
    let slice = application.inspect(|app| Ok(app.slice_named("todo")?.id()))?;
    let hide_done = application.entity_ref::<bool>(slice, "hide_done")?;
    let completed = TodoItem::type_descriptor().field_index("completed")?;
    let row = LivePart::Component(
        ComponentPart::typed(components.checkbox, todo_row).with_activation(mark_action),
    );
    let all = LivePart::ForLoop(ForLoopPart {
        expression: Expression::from_collection(todos),
        body: Box::new(row.clone()),
    });
    let incomplete = LivePart::ForLoop(ForLoopPart {
        expression: Expression::from_collection(todos),
        body: Box::new(LivePart::Match(MatchPart::new(
            Expression::field(completed),
            vec![MatchCandidate {
                pattern: MatchPattern::value(false),
                part: row,
            }],
        )?)),
    });
    Ok(UiDefinition::new(
        "todos",
        LivePart::Composite(CompositePart {
            parts: vec![
                LivePart::Component(ComponentPart::typed(components.label, heading)),
                LivePart::Component(
                    ComponentPart::typed(components.button, add_button).with_activation(add_action),
                ),
                LivePart::Component(ComponentPart::typed(comets, orbiting_comets::props)),
                LivePart::Component(
                    ComponentPart::typed(components.button, animation_button)
                        .with_activation(animation_action),
                ),
                LivePart::Component(logo),
                LivePart::Component(
                    ComponentPart::typed(components.checkbox, visibility_control)
                        .with_activation(visibility_action),
                ),
                LivePart::Match(MatchPart::new(
                    Expression::entity(hide_done),
                    vec![
                        MatchCandidate {
                            pattern: MatchPattern::value(false),
                            part: all,
                        },
                        MatchCandidate {
                            pattern: MatchPattern::value(true),
                            part: incomplete,
                        },
                    ],
                )?),
            ],
        }),
    )
    .with_window_properties(window_properties))
}

fn window_properties(
    context: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<pixui_engine::ui::window_properties::WindowProperties> {
    let app = context.application()?;
    let slice = app.slice_named("todo")?.id();
    let todos = app
        .collection(slice, "todos")?
        .arena::<TodoItem>()
        .ok_or_else(|| pixui_error!("wrong todo collection type"))?;
    let open = todos.iter().filter(|(_, todo)| !todo.completed).count();
    let title = if settings.locale == "de" {
        format!("Aufgaben — {open} offen")
    } else {
        format!("Todos — {open} open")
    };
    Ok(pixui_engine::ui::window_properties::WindowProperties {
        title: title.into(),
        icon: Some(pixui_engine::resources::path::ResourcePath::new(
            "images/pixui-logo.png",
        )?),
    })
}

fn animation_button(
    context: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<ButtonProps> {
    let app = context.application()?;
    let paused = *app.entity::<bool>(app.slice_named("todo")?.id(), "animation_paused")?;
    Ok(ButtonProps {
        label: match (settings.locale.as_str(), paused) {
            ("de", true) => "Animation fortsetzen",
            ("de", false) => "Animation pausieren",
            (_, true) => "Resume animation",
            (_, false) => "Pause animation",
        }
        .into(),
    })
}
fn animation_action(
    _: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    Ok(Box::new(|app| {
        app.action_call(app.slice_named("todo")?.id(), "toggle_animation", vec![])
    }))
}

fn hide_done(context: &ExpressionContext<'_>) -> PixuiResult<bool> {
    let app = context.application()?;
    Ok(*app.entity::<bool>(app.slice_named("todo")?.id(), "hide_done")?)
}
fn visibility_control(
    context: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<CheckboxProps> {
    Ok(CheckboxProps {
        checked: hide_done(context)?,
        label: if settings.locale == "de" {
            "Erledigte ausblenden"
        } else {
            "Hide completed"
        }
        .into(),
    })
}
fn visibility_action(
    _: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    Ok(Box::new(|app| {
        let slice = app.slice_named("todo")?;
        app.action_call(slice.id(), "toggle_hide_completed", vec![])
    }))
}

fn heading(
    context: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<LabelProps> {
    hide_done(context)?;
    Ok(LabelProps {
        text: if settings.locale == "de" {
            "Aufgaben"
        } else {
            "Todos"
        }
        .into(),
    })
}

fn add_button(
    _: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<ButtonProps> {
    Ok(ButtonProps {
        label: if settings.locale == "de" {
            "Aufgabe hinzufügen"
        } else {
            "Add todo"
        }
        .into(),
    })
}

fn add_action(
    _: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    let german = settings.locale == "de";
    Ok(Box::new(move |application| {
        let slice = application.slice_named("todo")?;
        let number = application
            .collection(slice.id(), "todos")?
            .arena::<TodoItem>()
            .ok_or_else(|| pixui_error!("wrong todo collection type"))?
            .len()
            + 1;
        let title = if german {
            format!("Neue Aufgabe {number}")
        } else {
            format!("New todo {number}")
        };
        application.action_call(slice.id(), "add_todo", vec![Box::new(title)])
    }))
}

fn todo_row(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<CheckboxProps> {
    let todo = context
        .value()?
        .downcast_ref::<TodoItem>()
        .ok_or_else(|| pixui_error!("todo row requires todo context"))?;
    Ok(CheckboxProps {
        label: todo.title.clone(),
        checked: todo.completed,
    })
}

fn mark_action(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    let application = context.application()?;
    let todo = context
        .value()?
        .downcast_ref::<TodoItem>()
        .ok_or_else(|| pixui_error!("todo row requires todo context"))?;
    let slice = application.slice_named("todo")?;
    let arena = application
        .collection(slice.id(), "todos")?
        .arena::<TodoItem>()
        .ok_or_else(|| pixui_error!("wrong todo collection type"))?;
    // Loop contexts expose values, not keys. Resolve by borrowed identity so
    // duplicate titles remain safe. Carrying keys through loops is future work.
    let (key, _) = arena
        .iter()
        .find(|(_, item)| std::ptr::eq(*item, todo))
        .ok_or_else(|| pixui_error!("todo no longer exists"))?;
    let reference = application.object_ref(slice.id(), "todos", key)?;
    let action = slice.action_handle_named("mark_done")?;
    Ok(Box::new(move |_| action.call(vec![Box::new(reference)])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::{actions::TodoActions, create_slice};
    use pixui_engine::ui::resource::Resource;
    use pixui_engine::{
        application::app::Application,
        ui::{
            display_list::DrawCommand,
            geometry::Point,
            input::{UiCommand, UiInput},
            presentation::Theme,
        },
    };
    use std::time::Duration;

    fn clear_settings_action() -> &'static pixui_engine::application::action::ActionDescriptor {
        static ACTION: std::sync::OnceLock<pixui_engine::application::action::ActionDescriptor> =
            std::sync::OnceLock::new();
        ACTION.get_or_init(|| {
            #[pixui_reflect::reflect(send)]
            mod args {
                pub struct Request {}
            }
            pixui_engine::application::action::ActionDescriptor::new::<args::Request>(
                "clear_settings",
                "Deletes the flag to verify stale binding failure",
                vec![],
                |app, slice, _| {
                    let reference = app.entity_ref::<bool>(slice, "hide_done")?;
                    let index = reference.collection_index();
                    app.resolve_collection_mut::<bool>(index)?.clear();
                    Ok(Box::new(()))
                },
            )
        })
    }

    #[test]
    fn stale_entity_keeps_last_good_render_and_geometry() {
        let app = Application::new();
        let slice = create_slice(&app).unwrap();
        app.register_action(slice, clear_settings_action()).unwrap();
        let components = app.register_standard_components().unwrap();
        app.register_standard_painters().unwrap();
        let comets = crate::orbiting_comets::register(&app).unwrap();
        let definition = app
            .register_ui(definition(&app, components, comets).unwrap())
            .unwrap();
        let (instance, outputs) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let initial = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        let bounds = app
            .inspect(move |app| {
                let bounds = app
                    .uis()
                    .instance(instance)?
                    .layout()
                    .component_bounds
                    .clone();
                Ok(bounds)
            })
            .unwrap();
        let call = app
            .action(slice, "clear_settings")
            .unwrap()
            .call(vec![])
            .unwrap();
        app.dispatch(call).unwrap().wait().unwrap();
        app.ui_command(UiCommand::Redraw { instance }).unwrap();
        app.inspect(move |app| {
            let ui = app.uis().instance(instance)?;
            assert_eq!(ui.revision(), initial.revision);
            assert_eq!(ui.layout().component_bounds, bounds);
            assert!(ui.last_error().unwrap().contains("stale"));
            Ok(())
        })
        .unwrap();
        assert!(outputs.try_recv().is_err());
    }

    #[test]
    fn visibility_is_shared_hides_rows_without_gaps_and_rejects_stale_clicks() {
        let app = Application::new();
        create_slice(&app).unwrap();
        let actions = TodoActions::bind(&app).unwrap();
        let first = actions.add_todo("Duplicate").unwrap();
        let second = actions.add_todo("Duplicate").unwrap();
        let reference = app.object_ref(actions.slice_id(), "todos", first).unwrap();
        actions.mark_done(reference).unwrap();
        let components = app.register_standard_components().unwrap();
        app.register_standard_painters().unwrap();
        let comets = crate::orbiting_comets::register(&app).unwrap();
        let definition = app
            .register_ui(definition(&app, components, comets).unwrap())
            .unwrap();
        let (one, outputs) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let (two, other) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let initial = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        other.recv_timeout(Duration::from_secs(2)).unwrap();
        actions.toggle_hide_completed().unwrap();
        let filtered = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        other.recv_timeout(Duration::from_secs(2)).unwrap();
        let point = app
            .inspect(move |app| {
                let a = app.uis().instance(one)?.layout();
                let b = app.uis().instance(two)?.layout();
                assert_eq!(a.hit_regions.len(), 4);
                assert_eq!(a.component_bounds.len(), 7);
                assert_eq!(a.component_bounds, b.component_bounds);
                let rect = a.hit_regions[3].bounds;
                Ok(Point {
                    x: rect.x + 1.0,
                    y: rect.y + 1.0,
                })
            })
            .unwrap();
        assert!(
            app.ui_command(UiCommand::Input {
                instance: one,
                revision: initial.revision,
                input: UiInput::MouseButton {
                    button: pixui_engine::ui::input::MouseButton::Left,
                    state: pixui_engine::ui::input::ButtonState::Released,
                    position: point,
                    modifiers: Default::default()
                }
            })
            .is_err()
        );
        app.ui_command(UiCommand::Input {
            instance: one,
            revision: filtered.revision,
            input: UiInput::MouseButton {
                button: pixui_engine::ui::input::MouseButton::Left,
                state: pixui_engine::ui::input::ButtonState::Released,
                position: point,
                modifiers: Default::default(),
            },
        })
        .unwrap();
        outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        other.recv_timeout(Duration::from_secs(2)).unwrap();
        app.inspect(move |app| {
            assert_eq!(app.uis().instance(one)?.layout().hit_regions.len(), 3);
            let todos = app
                .collection(app.slice_named("todo")?.id(), "todos")?
                .arena::<TodoItem>()
                .unwrap();
            assert_eq!(todos.len(), 2);
            assert!(todos.get(second).unwrap().completed);
            Ok(())
        })
        .unwrap();
        actions.toggle_hide_completed().unwrap();
        outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        other.recv_timeout(Duration::from_secs(2)).unwrap();
        app.inspect(move |app| {
            assert_eq!(app.uis().instance(one)?.layout().hit_regions.len(), 5);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn animation_replaces_snapshots_without_invalidating_presented_actions() {
        let app = Application::new();
        create_slice(&app).unwrap();
        let components = app.register_standard_components().unwrap();
        app.register_standard_painters().unwrap();
        let comets = crate::orbiting_comets::register(&app).unwrap();
        let definition = app
            .register_ui(definition(&app, components, comets).unwrap())
            .unwrap();
        let (instance, outputs) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let initial = outputs
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|error| {
                panic!(
                    "{error}: {:?}",
                    app.inspect(move |app| Ok(app
                        .uis()
                        .instance(instance)?
                        .last_error()
                        .map(str::to_owned)))
                        .unwrap()
                )
            });
        assert!(initial.animating);
        assert_eq!(initial.redraw_after, None);
        assert_eq!(initial.display_list.images.len(), 2);
        let pixels = initial.display_list.images[0]
            .rgb_pixels()
            .unwrap()
            .to_vec();
        for request in 1..=3 {
            app.ui_command(UiCommand::AnimationFrame { instance, request })
                .unwrap();
            // Leave the mailbox unread while newer outputs replace pending ones.
            app.inspect(|_| Ok(())).unwrap();
        }
        let latest = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(latest.revision.0 >= initial.revision.0 + 3);
        assert_eq!(latest.animation_request, Some(3));
        assert!(
            app.ui_command(UiCommand::AnimationFrame {
                instance,
                request: 3
            })
            .is_err()
        );
        assert_ne!(
            latest.display_list.images[0],
            initial.display_list.images[0]
        );
        assert_eq!(initial.display_list.images[0].rgb_pixels().unwrap(), pixels);
        let point = app
            .inspect(move |app| {
                let bounds = app.uis().instance(instance)?.layout().hit_regions[0].bounds;
                Ok(Point {
                    x: bounds.x + 1.0,
                    y: bounds.y + 1.0,
                })
            })
            .unwrap();
        app.ui_command(UiCommand::Input {
            instance,
            revision: initial.revision,
            input: UiInput::MouseButton {
                button: pixui_engine::ui::input::MouseButton::Left,
                state: pixui_engine::ui::input::ButtonState::Released,
                position: point,
                modifiers: Default::default(),
            },
        })
        .unwrap();
        let added = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(added.animation_request, Some(3));
        assert!(added.display_list.commands.iter().any(|command| matches!(command, DrawCommand::DrawText { text, .. } if text.contains("New todo"))));
        assert!(
            app.ui_command(UiCommand::Input {
                instance,
                revision: initial.revision,
                input: UiInput::MouseButton {
                    button: pixui_engine::ui::input::MouseButton::Left,
                    state: pixui_engine::ui::input::ButtonState::Released,
                    position: point,
                    modifiers: Default::default()
                }
            })
            .is_err()
        );
    }

    #[test]
    fn custom_button_painter_preserves_activation_and_changes_appearance() {
        let mut displays = Vec::new();
        for custom in [false, true] {
            let app = Application::new();
            create_slice(&app).unwrap();
            let components = app.register_standard_components().unwrap();
            if custom {
                app.register_painter::<pixui_engine::components::button::ButtonComponent>(
                    crate::custom_button_painter::CustomButtonPainter,
                )
                .unwrap();
                app.register_painter::<pixui_engine::components::label::LabelComponent>(
                    pixui_engine::painters::label::LabelPainter,
                )
                .unwrap();
                app.register_painter::<pixui_engine::components::checkbox::CheckboxComponent>(
                    pixui_engine::painters::checkbox::CheckboxPainter,
                )
                .unwrap();
                app.register_painter::<pixui_engine::components::image::ImageComponent>(
                    pixui_engine::painters::image::ImagePainter,
                )
                .unwrap();
            } else {
                app.register_standard_painters().unwrap();
            }
            let definition = app
                .register_ui(
                    definition(
                        &app,
                        components,
                        crate::orbiting_comets::register(&app).unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap();
            let (instance, outputs) = app
                .create_ui(definition, PresentationSettings::default())
                .unwrap();
            let output = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
            displays.push(output.display_list.clone());
            let point = app
                .inspect(move |app| {
                    let bounds = app.uis().instance(instance)?.layout().hit_regions[0].bounds;
                    Ok(Point {
                        x: bounds.x + 1.0,
                        y: bounds.y + 1.0,
                    })
                })
                .unwrap();
            app.ui_command(UiCommand::Input {
                instance,
                revision: output.revision,
                input: UiInput::PointerMoved(point),
            })
            .unwrap();
            let hovered = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(
                hovered
                    .display_list
                    .commands
                    .iter()
                    .any(|command| matches!(command, DrawCommand::StrokeRect { .. }))
            );
            app.ui_command(UiCommand::Input {
                instance,
                revision: hovered.revision,
                input: UiInput::PointerMoved(Point { x: 0.0, y: 0.0 }),
            })
            .unwrap();
            let unhovered = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
            app.ui_command(UiCommand::Input {
                instance,
                revision: unhovered.revision,
                input: UiInput::Keyboard(pixui_engine::ui::input::KeyboardEvent::named("Tab")),
            })
            .unwrap();
            let focused = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(
                focused
                    .display_list
                    .commands
                    .iter()
                    .any(|command| matches!(command, DrawCommand::StrokeRect { .. }))
            );
            app.ui_command(UiCommand::Input {
                instance,
                revision: focused.revision,
                input: UiInput::MouseButton {
                    button: pixui_engine::ui::input::MouseButton::Left,
                    state: pixui_engine::ui::input::ButtonState::Released,
                    position: point,
                    modifiers: Default::default(),
                },
            })
            .unwrap();
            let updated = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(updated.display_list.commands.iter().any(|command| matches!(command, DrawCommand::DrawText { text, .. } if text.contains("New todo"))));
            assert_eq!(
                app.inspect(move |app| Ok(app
                    .uis()
                    .instance(instance)?
                    .layout()
                    .hit_regions
                    .len()))
                    .unwrap(),
                4
            );
        }
        assert_ne!(displays[0], displays[1]);
    }

    #[test]
    fn two_instances_translate_and_share_actions_and_collection_data() {
        let application = Application::new();
        create_slice(&application).unwrap();
        let actions = TodoActions::bind(&application).unwrap();
        actions.add_todo("First").unwrap();
        let components = application.register_standard_components().unwrap();
        application.register_standard_painters().unwrap();
        let definition = application
            .register_ui(
                definition(
                    &application,
                    components,
                    crate::orbiting_comets::register(&application).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let (light, light_outputs) = application
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let (dark, dark_outputs) = application
            .create_ui(
                definition,
                PresentationSettings {
                    theme: Theme::Dark,
                    locale: "de".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let light_output = light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        let dark_output = dark_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(dark_output.display_list.commands.iter().any(|command| matches!(command, DrawCommand::DrawText { text, .. } if text.contains("Aufgaben"))));
        assert_ne!(light_output.display_list, dark_output.display_list);
        let point = application
            .inspect(move |app| {
                let rect = app.uis().instance(light)?.layout().hit_regions[0].bounds;
                Ok(Point {
                    x: rect.x + 1.0,
                    y: rect.y + 1.0,
                })
            })
            .unwrap();
        application
            .ui_command(UiCommand::Input {
                instance: light,
                revision: light_output.revision,
                input: UiInput::MouseButton {
                    button: pixui_engine::ui::input::MouseButton::Left,
                    state: pixui_engine::ui::input::ButtonState::Released,
                    position: point,
                    modifiers: Default::default(),
                },
            })
            .unwrap();
        let light_output = light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        dark_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        let (count, row) = application
            .inspect(move |app| {
                let instance = app.uis().instance(light)?;
                let rect = instance.layout().hit_regions[3].bounds;
                Ok((
                    instance.layout().hit_regions.len(),
                    Point {
                        x: rect.x + 1.0,
                        y: rect.y + 1.0,
                    },
                ))
            })
            .unwrap();
        assert_eq!(count, 5);
        application
            .ui_command(UiCommand::Input {
                instance: light,
                revision: light_output.revision,
                input: UiInput::MouseButton {
                    button: pixui_engine::ui::input::MouseButton::Left,
                    state: pixui_engine::ui::input::ButtonState::Released,
                    position: row,
                    modifiers: Default::default(),
                },
            })
            .unwrap();
        light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        dark_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            application
                .inspect(|app| Ok(app
                    .collection(app.slice_named("todo")?.id(), "todos")?
                    .arena::<TodoItem>()
                    .unwrap()
                    .iter()
                    .next()
                    .unwrap()
                    .1
                    .completed))
                .unwrap()
        );
        application
            .ui_command(UiCommand::Close { instance: dark })
            .unwrap();
        assert!(
            application
                .inspect(move |app| Ok(
                    app.uis().instance(light).is_ok() && app.uis().instance(dark).is_err()
                ))
                .unwrap()
        );
    }

    #[test]
    fn animated_windows_share_font_snapshots_and_new_todos_grow_them() {
        let application = Application::new();
        create_slice(&application).unwrap();
        let actions = TodoActions::bind(&application).unwrap();
        let components = application.register_standard_components().unwrap();
        application.register_standard_painters().unwrap();
        let definition = application
            .register_ui(
                definition(
                    &application,
                    components,
                    crate::orbiting_comets::register(&application).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let (light, light_outputs) = application
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let initial = light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        let (dark, dark_outputs) = application
            .create_ui(
                definition,
                PresentationSettings {
                    theme: Theme::Dark,
                    locale: "de".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let german = dark_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        application
            .ui_command(UiCommand::Redraw { instance: light })
            .unwrap();
        let shared = light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(Resource::ptr_eq(
            &shared.display_list.fonts[0],
            &german.display_list.fonts[0]
        ));
        for _ in 0..3 {
            application
                .ui_command(UiCommand::Redraw { instance: light })
                .unwrap();
            let animated = light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(Resource::ptr_eq(
                &shared.display_list.fonts[0],
                &animated.display_list.fonts[0]
            ));
        }
        let alphabet: String = (' '..='~').chain('\u{a0}'..='\u{24f}').collect();
        actions.add_todo(alphabet).unwrap();
        application.inspect(|_| Ok(())).unwrap();
        let grown = light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        let dark_grown = dark_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            grown.display_list.fonts[0].atlas().width()
                > initial.display_list.fonts[0].atlas().width()
        );
        assert!(Resource::ptr_eq(
            &grown.display_list.fonts[0],
            &dark_grown.display_list.fonts[0]
        ));
        assert!(
            initial.display_list.fonts[0].characters().len()
                < grown.display_list.fonts[0].characters().len()
        );
        initial.display_list.validate().unwrap();
        application
            .ui_command(UiCommand::Close { instance: light })
            .unwrap();
        application
            .ui_command(UiCommand::Close { instance: dark })
            .unwrap();
    }
    #[test]
    fn pause_button_stops_both_windows_and_resume_restarts_animation() {
        let app = Application::new();
        create_slice(&app).unwrap();
        let components = app.register_standard_components().unwrap();
        app.register_standard_painters().unwrap();
        let comets = crate::orbiting_comets::register(&app).unwrap();
        let definition = app
            .register_ui(definition(&app, components, comets).unwrap())
            .unwrap();
        let (one, outputs) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let (_, other) = app
            .create_ui(
                definition,
                PresentationSettings {
                    locale: "de".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let initial = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(initial.animating);
        assert!(
            other
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .animating
        );
        let point = app
            .inspect(move |app| {
                let rect = app.uis().instance(one)?.layout().hit_regions[1].bounds;
                Ok(Point {
                    x: rect.x + 1.0,
                    y: rect.y + 1.0,
                })
            })
            .unwrap();
        let click = |revision| {
            app.ui_command(UiCommand::Input {
                instance: one,
                revision,
                input: UiInput::MouseButton {
                    button: pixui_engine::ui::input::MouseButton::Left,
                    state: pixui_engine::ui::input::ButtonState::Released,
                    position: point,
                    modifiers: Default::default(),
                },
            })
            .unwrap()
        };
        click(initial.revision);
        let paused = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        let german = other.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!paused.animating && !german.animating);
        assert!(paused.display_list.commands.iter().any(
            |c| matches!(c, DrawCommand::DrawText { text, .. } if text == "Resume animation")
        ));
        assert!(german.display_list.commands.iter().any(
            |c| matches!(c, DrawCommand::DrawText { text, .. } if text == "Animation fortsetzen")
        ));
        app.ui_command(UiCommand::Redraw { instance: one }).unwrap();
        let redrawn = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!redrawn.animating);
        assert_eq!(
            paused.display_list.images[0],
            redrawn.display_list.images[0]
        );
        click(redrawn.revision);
        assert!(
            outputs
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .animating
        );
        assert!(
            other
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .animating
        );
    }
}
