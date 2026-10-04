//! A small component tree rendered as text, with persistent state across walks.

use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::{application_handle::ApplicationHandle, application_slice::SliceId},
    live_model::{
        part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
        state::{GenericComponentState, LiveState, PartState},
        walk::{Visitor, WalkEntry, walk},
    },
};
use pixui_reflect::{DynamicObject, FieldIndex, Reflect};

use crate::todo::TodoItem;

#[pixui_reflect::reflect]
mod model {
    pub struct TodoList {
        pub todos: Vec<Todo>,
    }

    pub struct Todo {
        pub title: String,
        pub completed: bool,
    }
}

/// Presentation state is owned, independent of the worker's application data.
enum ComponentState {
    Heading(String),
    Checkbox(bool),
    Label(String),
}

pub struct TodoUi {
    template: LivePart,
    state: LiveState,
}

impl TodoUi {
    pub fn new() -> Self {
        let heading = LivePart::Component(ComponentPart::new(|_| {
            Ok(GenericComponentState::new(ComponentState::Heading(
                "Todos".into(),
            )))
        }));
        let checkbox = LivePart::Component(ComponentPart::new(|_| {
            Ok(GenericComponentState::new(ComponentState::Checkbox(false)))
        }));
        let label = LivePart::Component(ComponentPart::new(|_| {
            Ok(GenericComponentState::new(ComponentState::Label(
                String::new(),
            )))
        }));
        let rows = LivePart::ForLoop(ForLoopPart {
            field_index: model::TodoList::type_descriptor()
                .field_index("todos")
                .expect("reflected todo list field")
                .0,
            body: Box::new(LivePart::Composite(CompositePart {
                parts: vec![checkbox, label],
            })),
        });
        Self {
            template: LivePart::Composite(CompositePart {
                parts: vec![heading, rows],
            }),
            state: LiveState::new(),
        }
    }

    /// Copies an owned snapshot, updates component state, and returns the physical tree.
    /// State persists by position; this example inserts items at the end.
    pub fn render(
        &mut self,
        application: &ApplicationHandle,
        slice: SliceId,
    ) -> PixuiResult<String> {
        let snapshot = application.inspect(move |state| {
            let todos = state
                .slice(slice)?
                .collection("todos")?
                .arena::<TodoItem>()
                .ok_or_else(|| pixui_error!("todos collection has the wrong item type"))?;
            Ok(model::TodoList {
                todos: todos
                    .iter()
                    .map(|(_, todo)| model::Todo {
                        title: todo.title.clone(),
                        completed: todo.completed,
                    })
                    .collect(),
            })
        })?;
        let context = DynamicObject::from_reflect(snapshot);
        let mut visitor = TreeVisitor::default();
        walk(
            &mut self.template,
            self.state.root_state_mut(),
            &context,
            &mut visitor,
        )?;
        Ok(visitor.output)
    }
}

/// Updates presentation values on every visit and prints physical nesting.
/// The stack records remaining children because the walker supplies preorder visits.
#[derive(Default)]
struct TreeVisitor {
    output: String,
    remaining_children: Vec<usize>,
}

impl Visitor for TreeVisitor {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
        while self.remaining_children.last() == Some(&0) {
            self.remaining_children.pop();
        }
        if let Some(remaining) = self.remaining_children.last_mut() {
            *remaining -= 1;
        }
        let indent = "  ".repeat(self.remaining_children.len());
        let (label, children) = match entry.state {
            PartState::Composite(state) => (
                format!("Composite ({} children)", state.parts.len()),
                state.parts.len(),
            ),
            PartState::ForLoop(_) => {
                let LivePart::ForLoop(part) = &*entry.part else {
                    return Err(pixui_error!("loop state requires a loop template"));
                };
                let count = entry
                    .context
                    .read_object(FieldIndex(part.field_index))?
                    .len()?;
                (format!("ForLoop ({count} todos)"), count)
            }
            PartState::Component(state) => {
                let component = state
                    .state
                    .downcast_mut::<ComponentState>()
                    .ok_or_else(|| pixui_error!("unexpected todo component state"))?;
                let label = match component {
                    ComponentState::Heading(title) => format!("Heading: {title}"),
                    ComponentState::Checkbox(checked) => {
                        let todo = entry
                            .context
                            .downcast_ref::<model::Todo>()
                            .ok_or_else(|| pixui_error!("checkbox requires todo context"))?;
                        *checked = todo.completed;
                        format!("Checkbox: [{}]", if *checked { "x" } else { " " })
                    }
                    ComponentState::Label(text) => {
                        let todo = entry
                            .context
                            .downcast_ref::<model::Todo>()
                            .ok_or_else(|| pixui_error!("label requires todo context"))?;
                        text.clone_from(&todo.title);
                        format!("Label: {text}")
                    }
                };
                (label, 0)
            }
            PartState::Unknown => return Err(pixui_error!("walker did not initialize state")),
        };
        self.output.push_str(&format!("{indent}{label}\n"));
        self.remaining_children.push(children);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::{actions::TodoActions, create_slice};
    use pixui_engine::application::app::Application;

    #[test]
    fn renders_existing_and_new_loop_items_and_refreshes_existing_component_state() {
        let application = Application::new();
        let slice = application.add_slice(create_slice().unwrap()).unwrap();
        let actions = TodoActions::bind(&application).unwrap();
        let mut ui = TodoUi::new();
        assert_eq!(
            ui.render(&application, slice).unwrap(),
            "Composite (2 children)\n  Heading: Todos\n  ForLoop (0 todos)\n"
        );
        let first = actions.add_todo("First task").unwrap();
        assert_eq!(
            ui.render(&application, slice).unwrap(),
            "Composite (2 children)\n  Heading: Todos\n  ForLoop (1 todos)\n    Composite (2 children)\n      Checkbox: [ ]\n      Label: First task\n"
        );
        actions
            .mark_done(application.object_ref(slice, "todos", first).unwrap())
            .unwrap();
        actions.add_todo("Second task").unwrap();
        assert_eq!(
            ui.render(&application, slice).unwrap(),
            "Composite (2 children)\n  Heading: Todos\n  ForLoop (2 todos)\n    Composite (2 children)\n      Checkbox: [x]\n      Label: First task\n    Composite (2 children)\n      Checkbox: [ ]\n      Label: Second task\n"
        );
        let PartState::Composite(root) = ui.state.root_state() else {
            panic!("root")
        };
        let PartState::ForLoop(rows) = &root.parts[1] else {
            panic!("loop")
        };
        assert_eq!(rows.items.len(), 2);
        assert!(
            rows.items
                .iter()
                .all(|row| matches!(row, PartState::Composite(_)))
        );
    }
}
