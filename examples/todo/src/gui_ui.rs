//! Typed component props and action bindings for the native todo example.

use crate::{
    orbiting_comets::{self, OrbitingComets},
    todo::TodoItem,
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    component_registry::component_id::ComponentId,
    components::{button::ButtonProps, checkbox::CheckboxProps, label::LabelProps},
    expression::{context::ExpressionContext, expression::Expression},
    live_model::part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
    painters::standard::StandardComponents,
    ui::{activation::ActionBinding, definition::UiDefinition, presentation::PresentationSettings},
};

pub fn definition(
    application: &ApplicationHandle,
    components: StandardComponents,
    comets: ComponentId<OrbitingComets>,
) -> PixuiResult<UiDefinition> {
    let todos = application.collection_key("todo", "todos")?;
    Ok(UiDefinition::new(
        "todos",
        LivePart::Composite(CompositePart {
            parts: vec![
                LivePart::Component(ComponentPart::typed(components.label, heading)),
                LivePart::Component(
                    ComponentPart::typed(components.button, add_button).with_activation(add_action),
                ),
                LivePart::Component(ComponentPart::typed(comets, orbiting_comets::props)),
                LivePart::ForLoop(ForLoopPart {
                    expression: Expression::from_collection(todos),
                    body: Box::new(LivePart::Component(
                        ComponentPart::typed(components.checkbox, todo_row)
                            .with_activation(mark_action),
                    )),
                }),
            ],
        }),
    ))
}

fn heading(_: &ExpressionContext<'_>, settings: &PresentationSettings) -> PixuiResult<LabelProps> {
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
        let number = slice
            .collection("todos")?
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
    let arena = slice
        .collection("todos")?
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

    #[test]
    fn animation_replaces_snapshots_without_invalidating_presented_actions() {
        let app = Application::new();
        app.add_slice(create_slice().unwrap()).unwrap();
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
        assert_eq!(initial.redraw_after, Some(Duration::from_millis(33)));
        assert_eq!(initial.display_list.images.len(), 1);
        let pixels = initial.display_list.images[0].pixels().to_vec();
        for _ in 0..3 {
            app.ui_command(UiCommand::Redraw { instance }).unwrap();
            // Leave the mailbox unread while newer outputs replace pending ones.
            app.inspect(|_| Ok(())).unwrap();
        }
        let latest = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(latest.revision.0 >= initial.revision.0 + 3);
        assert_ne!(
            latest.display_list.images[0],
            initial.display_list.images[0]
        );
        assert_eq!(initial.display_list.images[0].pixels(), pixels);
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
            input: UiInput::Activate(point),
        })
        .unwrap();
        let added = outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(added.display_list.commands.iter().any(|command| matches!(command, DrawCommand::DrawText { text, .. } if text.contains("New todo"))));
        assert!(
            app.ui_command(UiCommand::Input {
                instance,
                revision: initial.revision,
                input: UiInput::Activate(point)
            })
            .is_err()
        );
    }

    #[test]
    fn custom_button_painter_preserves_activation_and_changes_appearance() {
        let mut displays = Vec::new();
        for custom in [false, true] {
            let app = Application::new();
            app.add_slice(create_slice().unwrap()).unwrap();
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
                input: UiInput::FocusNext,
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
                input: UiInput::Activate(point),
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
                2
            );
        }
        assert_ne!(displays[0], displays[1]);
    }

    #[test]
    fn two_instances_translate_and_share_actions_and_collection_data() {
        let application = Application::new();
        application.add_slice(create_slice().unwrap()).unwrap();
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
                input: UiInput::Activate(point),
            })
            .unwrap();
        let light_output = light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        dark_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        let (count, row) = application
            .inspect(move |app| {
                let instance = app.uis().instance(light)?;
                let rect = instance.layout().hit_regions[1].bounds;
                Ok((
                    instance.layout().hit_regions.len(),
                    Point {
                        x: rect.x + 1.0,
                        y: rect.y + 1.0,
                    },
                ))
            })
            .unwrap();
        assert_eq!(count, 3);
        application
            .ui_command(UiCommand::Input {
                instance: light,
                revision: light_output.revision,
                input: UiInput::Activate(row),
            })
            .unwrap();
        light_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        dark_outputs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            application
                .inspect(|app| Ok(app
                    .slice_named("todo")?
                    .collection("todos")?
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
}
