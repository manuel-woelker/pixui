mod todo;

use pixui_base::{Key, PixuiResult};
use pixui_reflect::DynamicObject;
use todo::{TodoItem, create_application};

fn main() -> PixuiResult<()> {
    let mut application = create_application()?;
    let slice = application.slices[0].id();
    let call = application.action_call(
        slice,
        "add_todo",
        vec![DynamicObject::from_reflect(String::from(
            "Create a todo application",
        ))],
    )?;
    let key = *application
        .dispatch(call)?
        .downcast::<Key<TodoItem>>()
        .expect("add_todo returns a todo key");
    let todo = application.object_ref(slice, "todos", key)?;
    let call =
        application.action_call(slice, "mark_done", vec![DynamicObject::from_reflect(todo)])?;
    application.dispatch(call)?;
    let call = application.action_call(
        slice,
        "add_todo",
        vec![DynamicObject::from_reflect(String::from(
            "Add another task",
        ))],
    )?;
    application.dispatch(call)?;

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
