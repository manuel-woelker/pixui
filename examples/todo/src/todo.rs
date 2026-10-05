use pixui_base::{Arena, Key, PixuiResult, pixui_error};
use pixui_engine::application::{
    action::slice_actions, application_slice::ApplicationSlice, collection::Collection,
};

#[pixui_reflect::reflect]
pub mod model {
    /// Slice-wide presentation preferences, stored as one settings entry.
    pub struct TodoSettings {
        pub hide_completed: bool,
    }

    /// One task in the homogeneous todo collection.
    pub struct TodoItem {
        pub title: String,
        pub completed: bool,
    }
}

pub use model::{TodoItem, TodoSettings};

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

    /// Toggles shared completed-item visibility without modifying stored todos.
    #[action]
    pub fn toggle_hide_completed(settings: &mut Arena<TodoSettings>) -> PixuiResult<()> {
        if settings.len() != 1 {
            return Err(pixui_error!("todo settings must contain exactly one entry"));
        }
        let (_, settings) = settings.iter_mut().next().expect("one settings entry");
        settings.hide_completed = !settings.hide_completed;
        Ok(())
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
    let mut settings = Collection::new_reflected::<TodoSettings>("settings");
    settings
        .arena_mut::<TodoSettings>()
        .expect("settings arena")
        .insert(TodoSettings {
            hide_completed: false,
        });
    slice.add_collection(settings)?;
    actions::TodoActions::register(&mut slice)?;
    Ok(slice)
}

#[cfg(test)]
mod tests;
