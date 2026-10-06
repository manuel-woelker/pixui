//! Stable application-level collection storage and slice-local bindings.
use pixui_engine::application::{
    app::Application, application_slice::ApplicationSlice, collection::Collection,
};

#[test]
fn slices_share_storage_and_aliases_without_owning_collections() {
    let mut app = Application::default();
    let mut numbers = Collection::new::<i32>("numbers");
    let key = numbers.arena_mut::<i32>().unwrap().insert(42);
    let index = app.register_collection(numbers).unwrap();
    let mut first = ApplicationSlice::new("first");
    first.bind_collection("numbers", index).unwrap();
    let first = app.add_slice(first).unwrap();
    let second = app.add_slice(ApplicationSlice::new("second")).unwrap();
    app.bind_collection(second, "shared", index).unwrap();
    assert_eq!(app.collection_key("first", "numbers").unwrap(), index);
    assert_eq!(app.collection_key("second", "shared").unwrap(), index);
    assert!(std::ptr::eq(
        app.collection(first, "numbers").unwrap(),
        app.collection(second, "shared").unwrap()
    ));
    app.collection_mut::<i32>(second, "shared")
        .unwrap()
        .insert(7);
    assert_eq!(
        app.collection(first, "numbers")
            .unwrap()
            .arena::<i32>()
            .unwrap()
            .len(),
        2
    );
    let reference = app.object_ref(first, "numbers", key).unwrap();
    app.swap_slices(first, second).unwrap();
    assert_eq!(app.slice_index("second").unwrap(), 0);
    assert_eq!(app.slice_index("first").unwrap(), 1);
    app.remove_slice(first).unwrap();
    assert!(app.collection_key("first", "numbers").is_err());
    assert_eq!(*app.resolve_mut(reference).unwrap(), 42);
    assert_eq!(
        app.resolve_collection(index)
            .unwrap()
            .arena::<i32>()
            .unwrap()
            .len(),
        2
    );
    app.remove_slice(second).unwrap();
    // Even unbound collections remain alive until the application is dropped.
    assert_eq!(app.collections().len(), 1);
    assert_eq!(*app.resolve_mut(reference).unwrap(), 42);
    let replacement = app.add_slice(ApplicationSlice::new("first")).unwrap();
    app.bind_collection(replacement, "numbers", index).unwrap();
    assert_ne!(replacement, first);
    assert_eq!(app.collection_key("first", "numbers").unwrap(), index);
}

#[test]
fn appending_collections_preserves_indices_and_rejected_bindings_are_atomic() {
    let mut app = Application::default();
    let id = app.add_slice(ApplicationSlice::new("main")).unwrap();
    let index = app
        .add_collection(id, Collection::new::<i32>("numbers"))
        .unwrap();
    let other = app
        .add_collection(id, Collection::new::<String>("labels"))
        .unwrap();
    assert_ne!(index, other);
    assert!(
        app.add_collection(id, Collection::new::<bool>("numbers"))
            .is_err()
    );
    assert!(app.add_collection(id, Collection::new::<bool>("")).is_err());
    assert!(app.bind_collection(id, "numbers", other).is_err());
    assert!(app.bind_collection(id, "", other).is_err());
    assert!(
        app.register_collection(Collection::new::<bool>(""))
            .is_err()
    );
    assert_eq!(app.collections().len(), 2);
    assert_eq!(app.slice(id).unwrap().collections().len(), 2);
    assert_eq!(app.collection_key("main", "numbers").unwrap(), index);
    assert!(
        app.resolve_collection(index)
            .unwrap()
            .arena::<String>()
            .is_none()
    );
    assert!(app.resolve_collection_mut::<String>(index).is_err());
    assert!(app.collection_key("Main", "numbers").is_err());
    assert!(app.collection_key("main", "Numbers").is_err());
    assert!(app.add_slice(ApplicationSlice::new("main")).is_err());
    assert!(app.add_slice(ApplicationSlice::new("")).is_err());
    assert_eq!(app.slices().len(), 1);
}

#[test]
fn foreign_indices_and_slice_bindings_never_retarget_matching_positions() {
    let mut first = Application::default();
    let mut second = Application::default();
    let a = first
        .register_collection(Collection::new::<i32>("numbers"))
        .unwrap();
    let b = second
        .register_collection(Collection::new::<i32>("numbers"))
        .unwrap();
    assert_ne!(a, b);
    assert!(second.resolve_collection(a).is_err());
    assert!(second.resolve_collection_mut::<i32>(a).is_err());
    let mut foreign = ApplicationSlice::new("foreign");
    foreign.bind_collection("numbers", a).unwrap();
    assert!(second.add_slice(foreign).is_err());
    assert!(second.slices().is_empty());
    let id = second.add_slice(ApplicationSlice::new("local")).unwrap();
    assert!(second.bind_collection(id, "numbers", a).is_err());
    assert!(second.slice(id).unwrap().collections().is_empty());
    second.bind_collection(id, "numbers", b).unwrap();
}

#[test]
fn worker_handle_registers_and_shares_collections_by_index() {
    let app = Application::new();
    let index = app
        .register_collection(Collection::new::<String>("labels"))
        .unwrap();
    let mut slice = ApplicationSlice::new("main");
    slice.bind_collection("labels", index).unwrap();
    let id = app.add_slice(slice).unwrap();
    app.bind_collection(id, "alias", index).unwrap();
    assert_eq!(app.collection_key("main", "labels").unwrap(), index);
    assert_eq!(app.collection_key("main", "alias").unwrap(), index);
    app.inspect(move |state| {
        assert_eq!(state.resolve_collection(index)?.name(), "labels");
        assert_eq!(state.collections().len(), 1);
        Ok(())
    })
    .unwrap();
    assert!(app.collection_key("missing", "labels").is_err());
}
