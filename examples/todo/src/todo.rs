use pixui_base::{Arena, Key, PixuiResult, pixui_error};
use pixui_engine::application::{
    action::slice_actions,
    application_handle::ApplicationHandle,
    application_slice::{ApplicationSlice, SliceId},
    collection::Collection,
    entity_mut::EntityMut,
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

    /// Toggles shared completed-item visibility without modifying stored todos.
    #[action]
    pub fn toggle_hide_completed(mut hide_done: EntityMut<bool>) {
        *hide_done = !*hide_done;
    }

    /// Pauses or resumes animation in every window of the todo UI.
    #[action]
    pub fn toggle_animation(mut animation_paused: EntityMut<bool>) {
        *animation_paused = !*animation_paused;
    }

    /// Marks a todo complete. Repeated calls are harmless.
    /// Dispatch resolves the request's opaque todo reference before calling this function.
    #[action]
    pub fn mark_done(todo: &mut TodoItem) {
        todo.completed = true;
    }
}

/// Builds a todo slice with a named collection and validated action bindings.
pub fn create_slice(application: &ApplicationHandle) -> PixuiResult<SliceId> {
    let mut slice = ApplicationSlice::new("todo");
    let todos = application.register_collection(Collection::new_reflected::<TodoItem>("todos"))?;
    slice.bind_collection("todos", todos)?;
    slice.bind("hide_done", false)?;
    slice.bind("animation_paused", false)?;
    let id = application.add_slice(slice)?;
    actions::TodoActions::register(application, id)?;
    Ok(id)
}

#[cfg(test)]
mod tests;
