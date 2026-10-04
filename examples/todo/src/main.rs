mod todo;
mod ui;

use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::application::app::Application;
use todo::{actions::TodoActions, create_slice};
use ui::TodoUi;

fn main() -> PixuiResult<()> {
    let application = Application::new();
    application.add_slice(create_slice()?)?;
    let actions = TodoActions::bind(&application)?;
    let slice = actions.slice_id();
    let key = actions.add_todo("Create a todo application")?;
    let todo = application.object_ref(slice, "todos", key)?;
    let caller_actions = actions.clone();
    std::thread::spawn(move || caller_actions.mark_done(todo))
        .join()
        .map_err(|_| pixui_error!("todo caller panicked"))??;

    let mut ui = TodoUi::new();
    println!(
        "Before inserting a todo:\n{}",
        ui.render(&application, slice)?
    );
    actions.add_todo("Add another task")?;
    println!(
        "After inserting a todo:\n{}",
        ui.render(&application, slice)?
    );
    Ok(())
}
