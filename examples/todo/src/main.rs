mod todo;

use pixui_base::{Key, PixuiResult, pixui_error};
use pixui_engine::application::dispatch::Dispatch;
use todo::{TodoItem, create_application};

fn main() -> PixuiResult<()> {
    let mut application = create_application()?;
    let slice = application.slices[0].id();
    let call = application.action_call(
        slice,
        "add_todo",
        vec![Box::new(String::from("Create a todo application"))],
    )?;
    let key = *application
        .dispatch(call)?
        .downcast::<Key<TodoItem>>()
        .expect("add_todo returns a todo key");
    let todo = application.object_ref(slice, "todos", key)?;
    let mark_done = application.action_call(slice, "mark_done", vec![Box::new(todo)])?;
    let add_todo = application.action_call(
        slice,
        "add_todo",
        vec![Box::new(String::from("Add another task"))],
    )?;

    let (dispatch, owner) = Dispatch::new(16);
    let worker = std::thread::spawn(move || owner.run(application));
    let caller_dispatch = dispatch.clone();
    let caller = std::thread::spawn(move || caller_dispatch.dispatch(mark_done)?.wait());
    dispatch.dispatch(add_todo)?.wait()?;
    caller
        .join()
        .map_err(|_| pixui_error!("todo caller panicked"))??;
    // Closing every producer drains accepted calls and returns the application.
    drop(dispatch);
    let application = worker
        .join()
        .map_err(|_| pixui_error!("dispatch owner panicked"))?;

    let slice = application.slice(slice)?;
    println!("{}:", slice.name);
    let todos = slice
        .collection("todos")?
        .arena::<TodoItem>()
        .expect("todo collection");
    for (_, item) in todos.iter() {
        let marker = if item.completed { "x" } else { " " };
        println!("  [{marker}] {}", item.title);
    }
    Ok(())
}
