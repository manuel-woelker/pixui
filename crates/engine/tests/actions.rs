use pixui_base::Key;
use pixui_engine::application::{
    action::{ActionCall, ActionIndex, action},
    app::Application,
    application_slice::ApplicationSlice,
    collection::Collection,
};

/// Replaces the value of a resolved item.
#[action]
fn replace(request: &mut i32, application: i32, slice: i32) -> i32 {
    let previous = *request;
    *request = application + slice;
    previous
}

#[action]
fn answer() -> i32 {
    42
}

#[action]
fn sum(application: i32, slice: i32, request: i32) -> i32 {
    application + slice + request
}

#[cfg_attr(all(), cfg(any()))]
#[action]
fn unavailable(todos: &mut pixui_base::Arena<i32>) {
    todos.clear();
}

#[test]
fn dispatches_without_collection_injection_and_avoids_parameter_name_collisions() {
    let mut application = Application::default();
    let slice = application
        .add_slice(ApplicationSlice::new("numbers"))
        .unwrap();
    application
        .add_collection(slice, Collection::new::<i32>("numbers"))
        .unwrap();
    application
        .register_action(slice, replace_action::descriptor())
        .unwrap();
    application
        .register_action(slice, answer_action::descriptor())
        .unwrap();
    application
        .register_action(slice, sum_action::descriptor())
        .unwrap();
    let id = slice;
    let key: Key<i32> = application
        .collection_mut::<i32>(slice, "numbers")
        .unwrap()
        .insert(1);
    let reference = application.object_ref(id, "numbers", key).unwrap();
    let call = application
        .action_call(
            id,
            "replace",
            vec![Box::new(reference), Box::new(2i32), Box::new(3i32)],
        )
        .unwrap();
    assert_eq!(
        *application
            .dispatch(call)
            .unwrap()
            .downcast::<i32>()
            .unwrap(),
        1
    );
    assert_eq!(*application.resolve_mut(reference).unwrap(), 5);
    let call = application.action_call(id, "answer", vec![]).unwrap();
    assert_eq!(
        *application
            .dispatch(call)
            .unwrap()
            .downcast::<i32>()
            .unwrap(),
        42
    );
    let call = application
        .action_call(
            id,
            "sum",
            vec![Box::new(1i32), Box::new(2i32), Box::new(3i32)],
        )
        .unwrap();
    assert_eq!(
        *application
            .dispatch(call)
            .unwrap()
            .downcast::<i32>()
            .unwrap(),
        6
    );
    assert!(
        application
            .dispatch(ActionCall {
                slice: id,
                action: ActionIndex(99),
                request: Box::new(false),
            })
            .is_err()
    );
}

#[test]
fn rejects_keys_from_another_collection_and_checks_object_type() {
    let mut application = Application::default();
    let slice = application
        .add_slice(ApplicationSlice::new("numbers"))
        .unwrap();
    application
        .add_collection(slice, Collection::new::<i32>("numbers"))
        .unwrap();
    application
        .add_collection(slice, Collection::new::<i32>("other"))
        .unwrap();
    application
        .add_collection(slice, Collection::new::<String>("strings"))
        .unwrap();
    application
        .register_action(slice, replace_action::descriptor())
        .unwrap();
    let id = slice;
    let key = application
        .collection_mut::<i32>(slice, "numbers")
        .unwrap()
        .insert(1);
    let string_key = application
        .collection_mut::<String>(slice, "strings")
        .unwrap()
        .insert("text".into());
    assert!(application.object_ref(id, "other", key).is_err());
    assert!(application.object_ref(id, "strings", key).is_err());
    assert!(application.object_ref(id, "missing", key).is_err());
    let string = application.object_ref(id, "strings", string_key).unwrap();
    assert!(
        application
            .action_call(
                id,
                "replace",
                vec![Box::new(string), Box::new(2i32), Box::new(3i32),]
            )
            .is_err()
    );
    assert_eq!(
        *application
            .collection_mut::<i32>(id, "numbers")
            .unwrap()
            .get(key)
            .unwrap(),
        1
    );
}
