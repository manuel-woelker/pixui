mod todo;

use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::application::app::Application;
use todo::{TodoItem, actions::TodoActions, create_slice};

fn main() -> PixuiResult<()> {
    let application = Application::new();
    application.add_slice(create_slice()?)?;
    let actions = TodoActions::bind(&application)?;
    let slice = actions.slice_id();
    let key = actions.add_todo("Create a todo application")?;
    let todo = application.object_ref(slice, "todos", key)?;
    let caller_actions = actions.clone();
    let caller = std::thread::spawn(move || caller_actions.mark_done(todo));
    actions.add_todo("Add another task")?;
    caller
        .join()
        .map_err(|_| pixui_error!("todo caller panicked"))??;

    let todos = application.inspect(move |state| {
        let todos = state
            .slice(slice)?
            .collection("todos")?
            .arena::<TodoItem>()
            .expect("todo collection");
        Ok(todos
            .iter()
            .map(|(_, item)| (item.title.clone(), item.completed))
            .collect::<Vec<_>>())
    })?;
    println!("todo:");
    for (title, completed) in todos {
        let marker = if completed { "x" } else { " " };
        println!("  [{marker}] {title}");
    }
    Ok(())
}
