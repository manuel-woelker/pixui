use pixui_base::{Arena, Key, PixuiResult, pixui_error};
use pixui_engine::application::{
    action::slice_actions, application_slice::ApplicationSlice, collection::Collection,
};

#[pixui_reflect::reflect]
pub mod model {
    /// One task in the homogeneous todo collection.
    pub struct TodoItem {
        pub title: String,
        pub completed: bool,
    }
}

pub use model::TodoItem;

#[slice_actions(slice = "todo", facade = TodoActions)]
pub mod actions {
    use super::*;

    /// Creates an incomplete todo with a nonblank title.
    /// The `todos` collection is injected by name when dispatching.
    #[action]
    pub fn add_todo(todos: &mut Arena<TodoItem>, title: String) -> PixuiResult<Key<TodoItem>> {
        if title.trim().is_empty() {
            return Err(pixui_error!("todo title must not be blank"));
        }
        Ok(todos.insert(TodoItem {
            title,
            completed: false,
        }))
    }

    /// Marks a todo complete. Repeated calls are harmless.
    /// Dispatch resolves the request's opaque todo reference before calling this function.
    #[action]
    pub fn mark_done(todo: &mut TodoItem) {
        todo.completed = true;
    }
}

/// Builds a todo slice with a named collection and validated action bindings.
pub fn create_slice() -> PixuiResult<ApplicationSlice> {
    let mut slice = ApplicationSlice::new("todo");
    slice.add_collection(Collection::new_reflected::<TodoItem>("todos"))?;
    actions::TodoActions::register(&mut slice)?;
    Ok(slice)
}

#[cfg(test)]
mod tests;
