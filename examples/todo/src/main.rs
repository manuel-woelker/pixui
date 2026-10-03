mod todo;

use pixui_base::{Key, PixuiResult, pixui_error};
use pixui_engine::application::app::Application;
use todo::{TodoItem, create_slice};

fn main() -> PixuiResult<()> {
    let application = Application::new();
    let todo_slice = create_slice()?;
    let add_todo = todo_slice.action_handle_named("add_todo")?;
    let mark_done = todo_slice.action_handle_named("mark_done")?;
    let slice = application.add_slice(todo_slice)?;
    let call = add_todo.call(vec![Box::new(String::from("Create a todo application"))])?;
    let key = *application
        .dispatch(call)?
        .wait()?
        .downcast::<Key<TodoItem>>()
        .expect("add_todo returns a todo key");
    let todo = application.object_ref(slice, "todos", key)?;
    let mark_done_call = mark_done.call(vec![Box::new(todo)])?;
    let caller_handle = application.clone();
    let caller = std::thread::spawn(move || caller_handle.dispatch(mark_done_call)?.wait());
    let call = add_todo.call(vec![Box::new(String::from("Add another task"))])?;
    application.dispatch(call)?.wait()?;
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
