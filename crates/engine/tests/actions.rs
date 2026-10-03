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
    let mut slice = ApplicationSlice::new("numbers");
    slice
        .add_collection(Collection::new::<i32>("numbers"))
        .unwrap();
    slice.register_action(replace_action::descriptor()).unwrap();
    slice.register_action(answer_action::descriptor()).unwrap();
    slice.register_action(sum_action::descriptor()).unwrap();
    let id = slice.id();
    let key: Key<i32> = slice.collection_mut::<i32>("numbers").unwrap().insert(1);
    let mut application = Application::new();
    application.slices.push(slice);
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
    let mut slice = ApplicationSlice::new("numbers");
    slice
        .add_collection(Collection::new::<i32>("numbers"))
        .unwrap();
    slice
        .add_collection(Collection::new::<i32>("other"))
        .unwrap();
    slice
        .add_collection(Collection::new::<String>("strings"))
        .unwrap();
    slice.register_action(replace_action::descriptor()).unwrap();
    let id = slice.id();
    let key = slice.collection_mut::<i32>("numbers").unwrap().insert(1);
    let string_key = slice
        .collection_mut::<String>("strings")
        .unwrap()
        .insert("text".into());
    let mut application = Application::new();
    application.slices.push(slice);
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
