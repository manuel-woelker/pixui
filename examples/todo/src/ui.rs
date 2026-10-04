//! A small component tree rendered as text, with persistent state across walks.

use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::{application_handle::ApplicationHandle, application_slice::SliceId},
    expression::{context::ExpressionContext, evaluator::evaluate, expression::Expression},
    live_model::{
        part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
        state::{GenericComponentState, LiveState, PartState},
        walk::{Visitor, WalkEntry, walk},
    },
};

use crate::todo::TodoItem;

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
    pub fn new(application: &ApplicationHandle, slice: SliceId) -> PixuiResult<Self> {
        let collection_index = application.inspect(move |state| {
            state
                .slice(slice)?
                .collections()
                .iter()
                .position(|collection| collection.name() == "todos")
                .ok_or_else(|| pixui_error!("unknown todos collection"))
        })?;
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
            expression: Expression::collection(slice, collection_index),
            body: Box::new(LivePart::Composite(CompositePart {
                parts: vec![checkbox, label],
            })),
        });
        Ok(Self {
            template: LivePart::Composite(CompositePart {
                parts: vec![heading, rows],
            }),
            state: LiveState::new(),
        })
    }

    /// Walks against live application collections on their owner thread.
    /// Moves the owned UI to the worker and back; only output text escapes borrowing.
    /// State persists by position; this example inserts items at the end.
    /// A worker communication failure discards UI state along with the transferred UI.
    pub fn render(&mut self, application: &ApplicationHandle) -> PixuiResult<String> {
        let mut ui = std::mem::replace(
            self,
            Self {
                template: LivePart::Composite(CompositePart { parts: vec![] }),
                state: LiveState::new(),
            },
        );
        let (ui, result) = application.inspect(move |state| {
            let mut visitor = TreeVisitor::default();
            let result = walk(
                &mut ui.template,
                ui.state.root_state_mut(),
                &ExpressionContext::new(state),
                &mut visitor,
            )
            .map(|_| visitor.output);
            Ok((ui, result))
        })?;
        *self = ui;
        result
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
                let count = evaluate(entry.context, &part.expression)?.len()?;
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
                            .value()?
                            .downcast_ref::<TodoItem>()
                            .ok_or_else(|| pixui_error!("checkbox requires todo context"))?;
                        *checked = todo.completed;
                        format!("Checkbox: [{}]", if *checked { "x" } else { " " })
                    }
                    ComponentState::Label(text) => {
                        let todo = entry
                            .context
                            .value()?
                            .downcast_ref::<TodoItem>()
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
        let mut ui = TodoUi::new(&application, slice).unwrap();
        assert_eq!(
            ui.render(&application).unwrap(),
            "Composite (2 children)\n  Heading: Todos\n  ForLoop (0 todos)\n"
        );
        let first = actions.add_todo("First task").unwrap();
        assert_eq!(
            ui.render(&application).unwrap(),
            "Composite (2 children)\n  Heading: Todos\n  ForLoop (1 todos)\n    Composite (2 children)\n      Checkbox: [ ]\n      Label: First task\n"
        );
        actions
            .mark_done(application.object_ref(slice, "todos", first).unwrap())
            .unwrap();
        actions.add_todo("Second task").unwrap();
        assert_eq!(
            ui.render(&application).unwrap(),
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

#[cfg(test)]
mod collection_tests {
    use super::*;
    use crate::todo::actions::TodoActions;
    use pixui_engine::{
        application::{
            app::Application, application_slice::ApplicationSlice, collection::Collection,
        },
        expression::expression::ExpressionKind,
    };

    #[test]
    fn resolves_todos_collection_by_name_and_uses_a_collection_expression() {
        let application = Application::new();
        let mut slice = ApplicationSlice::new("alternative");
        slice
            .add_collection(Collection::new::<String>("metadata"))
            .unwrap();
        slice
            .add_collection(Collection::new_reflected::<TodoItem>("todos"))
            .unwrap();
        TodoActions::register(&mut slice).unwrap();
        let id = application.add_slice(slice).unwrap();
        let actions = TodoActions::bind_to(&application, id).unwrap();
        actions.add_todo("From the collection").unwrap();
        let mut ui = TodoUi::new(&application, id).unwrap();
        let LivePart::Composite(root) = &ui.template else {
            panic!("root")
        };
        let LivePart::ForLoop(rows) = &root.parts[1] else {
            panic!("loop")
        };
        let ExpressionKind::Collection(expression) = rows.expression.kind() else {
            panic!("collection expression")
        };
        assert_eq!(expression.slice_id(), id);
        assert_eq!(expression.collection_index(), 1);
        let output = ui.render(&application).unwrap();
        assert!(output.contains("ForLoop (1 todos)"));
        assert!(output.contains("Label: From the collection"));
    }
}
