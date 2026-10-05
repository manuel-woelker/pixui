use super::actions::*;
use super::*;
use pixui_engine::application::{
    action::{ActionCall, ActionIndex},
    application_slice::SliceId,
};
use pixui_reflect::Reflect;

use pixui_engine::application::app::Application;

fn create_application() -> PixuiResult<Application> {
    let mut application = Application::default();
    application.add_slice(create_slice()?)?;
    Ok(application)
}

fn add(application: &mut Application, slice: SliceId, title: &str) -> Key<TodoItem> {
    let call = application
        .action_call(slice, "add_todo", vec![Box::new(title.to_owned())])
        .unwrap();
    *application
        .dispatch(call)
        .unwrap()
        .downcast::<Key<TodoItem>>()
        .unwrap()
}

#[test]
fn discovers_constructs_and_dispatches_owned_requests() {
    let mut application = create_application().unwrap();
    let slice = application.slices()[0].id();
    let action = application
        .slice(slice)
        .unwrap()
        .action_named("add_todo")
        .unwrap();
    assert!(!action.description().is_empty());
    assert_eq!(
        action
            .arguments()
            .fields()
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        ["title"]
    );
    assert_eq!(action.collections()[0].name, "todos");
    assert_eq!(
        action.collections()[0].item_type_id,
        std::any::TypeId::of::<TodoItem>()
    );
    let done = application
        .slice(slice)
        .unwrap()
        .action_named("mark_done")
        .unwrap();
    assert_eq!(done.arguments().fields()[0].name, "todo");
    assert_eq!(done.arguments().fields().len(), 1);
    assert!(done.collections().is_empty());

    let key = add(&mut application, slice, "Buy milk");
    let reference = application.object_ref(slice, "todos", key).unwrap();
    assert_eq!(
        application.resolve_mut(reference).unwrap().title,
        "Buy milk"
    );
    assert!(!application.resolve_mut(reference).unwrap().completed);
    // Request ownership allows queueing while application data changes.
    let call = application
        .action_call(slice, "mark_done", vec![Box::new(reference)])
        .unwrap();
    add(&mut application, slice, "Another task");
    assert!(application.dispatch(call).unwrap().is::<()>());
    let call = application
        .action_call(slice, "mark_done", vec![Box::new(reference)])
        .unwrap();
    application.dispatch(call).unwrap();
    assert!(application.resolve_mut(reference).unwrap().completed);
}

#[test]
fn registration_requires_named_collection_with_correct_type() {
    let descriptor = add_todo_action::descriptor();
    let mut slice = ApplicationSlice::new("missing");
    assert!(slice.register_action(descriptor).is_err());
    assert!(slice.actions().is_empty());
    slice
        .add_collection(Collection::new::<TodoItem>("other"))
        .unwrap();
    assert!(slice.register_action(descriptor).is_err());
    slice
        .add_collection(Collection::new::<String>("todos"))
        .unwrap();
    assert!(slice.register_action(descriptor).is_err());
    assert!(slice.actions().is_empty());

    let mut slice = ApplicationSlice::new("valid");
    slice
        .add_collection(Collection::new::<TodoItem>("todos"))
        .unwrap();
    slice
        .add_collection(Collection::new::<TodoItem>("archived"))
        .unwrap();
    assert!(
        slice
            .add_collection(Collection::new::<TodoItem>("todos"))
            .is_err()
    );
    assert!(
        slice
            .add_collection(Collection::new::<TodoItem>(""))
            .is_err()
    );
    assert_eq!(slice.collections().len(), 2);
    assert_eq!(slice.register_action(descriptor).unwrap(), ActionIndex(0));
    assert!(slice.register_action(descriptor).is_err());
    assert_eq!(slice.actions().len(), 1);
    assert!(slice.action_named("unknown").is_err());
    assert!(slice.action(ActionIndex(99)).is_err());
}

#[test]
fn parameter_name_disambiguates_collections_with_same_item_type() {
    let mut application = create_application().unwrap();
    let slice = application.slices()[0].id();
    application
        .add_collection(slice, Collection::new::<TodoItem>("archived"))
        .unwrap();
    add(&mut application, slice, "task");
    assert_eq!(
        application
            .slice(slice)
            .unwrap()
            .collection("todos")
            .unwrap()
            .arena::<TodoItem>()
            .unwrap()
            .len(),
        1
    );
    assert!(
        application
            .slice(slice)
            .unwrap()
            .collection("archived")
            .unwrap()
            .arena::<TodoItem>()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_requests_and_titles_do_not_mutate_the_collection() {
    let mut application = create_application().unwrap();
    let slice = application.slices()[0].id();
    assert!(application.action_call(slice, "add_todo", vec![]).is_err());
    assert!(
        application
            .action_call(slice, "add_todo", vec![Box::new(false)])
            .is_err()
    );
    assert!(application.action_call(slice, "unknown", vec![]).is_err());
    let action = application
        .slice(slice)
        .unwrap()
        .action_index("add_todo")
        .unwrap();
    assert!(
        application
            .dispatch(ActionCall {
                slice,
                action,
                request: Box::new(String::new())
            })
            .is_err()
    );
    static REQUEST: std::sync::OnceLock<add_todo_action::request::Request> =
        std::sync::OnceLock::new();
    let request = REQUEST.get_or_init(|| add_todo_action::request::Request {
        title: "borrowed".into(),
    });
    assert!(
        application
            .dispatch(ActionCall {
                slice,
                action,
                request: Box::new(request)
            })
            .is_err()
    );
    let call = application
        .action_call(slice, "add_todo", vec![Box::new("  ".to_owned())])
        .unwrap();
    assert!(application.dispatch(call).is_err());
    assert!(
        application
            .slice(slice)
            .unwrap()
            .collection("todos")
            .unwrap()
            .arena::<TodoItem>()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn stale_foreign_and_removed_slice_references_are_rejected() {
    let mut application = create_application().unwrap();
    let slice = application.slices()[0].id();
    let key = add(&mut application, slice, "task");
    let reference = application.object_ref(slice, "todos", key).unwrap();
    let call = application
        .action_call(slice, "mark_done", vec![Box::new(reference)])
        .unwrap();
    let mut other = create_application().unwrap();
    let other_slice = other.slices()[0].id();
    assert!(other.resolve_mut(reference).is_err());
    assert!(other.object_ref(other_slice, "todos", key).is_err());
    application
        .collection_mut::<TodoItem>(slice, "todos")
        .unwrap()
        .remove(key);
    assert!(application.dispatch(call).is_err());
    assert!(application.object_ref(slice, "todos", key).is_err());
    let key = add(&mut application, slice, "new task");
    let reference = application.object_ref(slice, "todos", key).unwrap();
    let call = application
        .action_call(slice, "mark_done", vec![Box::new(reference)])
        .unwrap();
    application.remove_slice(slice).unwrap();
    assert!(application.dispatch(call).is_err());
    assert!(application.resolve_mut(reference).is_err());
}

#[test]
fn handles_and_calls_survive_slice_reordering() {
    let mut application = create_application().unwrap();
    let slice = application.slices()[0].id();
    let key = add(&mut application, slice, "task");
    let reference = application.object_ref(slice, "todos", key).unwrap();
    let call = application
        .action_call(slice, "mark_done", vec![Box::new(reference)])
        .unwrap();
    let other = application
        .add_slice(ApplicationSlice::new("other"))
        .unwrap();
    application.swap_slices(slice, other).unwrap();
    application.dispatch(call).unwrap();
    assert!(application.resolve_mut(reference).unwrap().completed);
    assert!(
        !pixui_engine::application::object_ref::ObjectRef::<TodoItem>::type_descriptor()
            .is_constructible()
    );
}

#[test]
fn handlers_are_ordinary_functions_with_direct_mutable_arguments() {
    let mut todos = Arena::new();
    let key = add_todo(&mut todos, "task".into()).unwrap();
    let todo = todos.get_mut(key).unwrap();
    mark_done(todo);
    assert!(todo.completed);
}

#[test]
fn visibility_toggle_requires_a_single_settings_entry_and_has_no_request_fields() {
    let mut app = create_application().unwrap();
    let slice = app.slices()[0].id();
    assert!(
        app.slice(slice)
            .unwrap()
            .action_named("toggle_hide_completed")
            .unwrap()
            .arguments()
            .fields()
            .is_empty()
    );
    for expected in [true, false, true] {
        let call = app
            .action_call(slice, "toggle_hide_completed", vec![])
            .unwrap();
        app.dispatch(call).unwrap();
        assert_eq!(
            app.collection_mut::<TodoSettings>(slice, "settings")
                .unwrap()
                .iter()
                .next()
                .unwrap()
                .1
                .hide_completed,
            expected
        );
    }
    let settings = app
        .collection_mut::<TodoSettings>(slice, "settings")
        .unwrap();
    settings.insert(TodoSettings {
        hide_completed: false,
    });
    assert!(toggle_hide_completed(settings).is_err());
    settings.clear();
    assert!(toggle_hide_completed(settings).is_err());
}
