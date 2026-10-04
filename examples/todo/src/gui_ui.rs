//! Native todo presentation built from the same live-part model as the text UI.

use crate::todo::TodoItem;
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    expression::{context::ExpressionContext, expression::Expression},
    live_model::part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
    ui::{definition::UiDefinition, presentation::PresentationSettings, widget::Widget},
};

pub fn definition(application: &ApplicationHandle) -> PixuiResult<UiDefinition> {
    let todos = application.collection_key("todo", "todos")?;
    let component = |presentation| {
        LivePart::Component(ComponentPart::default().with_presentation(presentation))
    };
    Ok(UiDefinition::new(
        "todos",
        LivePart::Composite(CompositePart {
            parts: vec![
                component(heading),
                component(add_button),
                LivePart::ForLoop(ForLoopPart {
                    expression: Expression::from_collection(todos),
                    body: Box::new(component(todo_row)),
                }),
            ],
        }),
    ))
}

fn heading(_: &ExpressionContext<'_>, settings: &PresentationSettings) -> PixuiResult<Widget> {
    Ok(Widget::Label {
        text: if settings.locale == "de" {
            "Aufgaben"
        } else {
            "Todos"
        }
        .into(),
    })
}

fn add_button(_: &ExpressionContext<'_>, settings: &PresentationSettings) -> PixuiResult<Widget> {
    let german = settings.locale == "de";
    Ok(Widget::Button {
        text: if german {
            "Aufgabe hinzufügen"
        } else {
            "Add todo"
        }
        .into(),
        activate: Box::new(move |application| {
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
        }),
    })
}

fn todo_row(context: &ExpressionContext<'_>, _: &PresentationSettings) -> PixuiResult<Widget> {
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
    // The reflected loop currently supplies values without arena keys. Resolve
    // the borrowed item's key by identity; titles may be duplicated. This is a
    // linear lookup per row, suitable for this small example.
    let (key, _) = arena
        .iter()
        .find(|(_, item)| std::ptr::eq(*item, todo))
        .ok_or_else(|| pixui_error!("todo no longer exists"))?;
    let reference = application.object_ref(slice.id(), "todos", key)?;
    let action = slice.action_handle_named("mark_done")?;
    Ok(Widget::Checkbox {
        text: todo.title.clone(),
        checked: todo.completed,
        activate: Box::new(move |_| action.call(vec![Box::new(reference)])),
    })
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
    fn two_instances_translate_and_share_actions_and_collection_data() {
        let application = Application::new();
        application.add_slice(create_slice().unwrap()).unwrap();
        let actions = TodoActions::bind(&application).unwrap();
        actions.add_todo("First").unwrap();
        let definition = application
            .register_ui(definition(&application).unwrap())
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
