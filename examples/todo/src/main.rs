use pixui_engine::application::{
    app::Application, application_slice::ApplicationSlice, collection::Collection,
};

/// One task in the todo collection.
struct TodoItem {
    title: String,
    completed: bool,
}

/// Builds a todo slice with a homogeneous collection of sample tasks.
fn create_application() -> Application {
    let mut items = Collection::new::<TodoItem>("items");
    let arena = items
        .arena_mut::<TodoItem>()
        .expect("the collection was created for TodoItem");
    arena.insert(TodoItem {
        title: "Create a todo application".into(),
        completed: true,
    });
    arena.insert(TodoItem {
        title: "Add another task".into(),
        completed: false,
    });

    let mut todo = ApplicationSlice::new("todo");
    todo.collections.push(items);

    let mut application = Application::new();
    application.slices.push(todo);
    application
}

fn main() {
    let application = create_application();
    for slice in &application.slices {
        println!("{}:", slice.name);
        for collection in &slice.collections {
            let items = collection
                .arena::<TodoItem>()
                .expect("the todo slice contains TodoItem collections");
            for (_, item) in items.iter() {
                let marker = if item.completed { "x" } else { " " };
                println!("  [{marker}] {}", item.title);
            }
        }
    }
}
