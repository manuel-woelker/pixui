use pixui_engine::application::{
    app::Application, application_slice::ApplicationSlice, collection::Collection,
    collection_key::CollectionKey,
};

fn slice(name: &str) -> ApplicationSlice {
    let mut slice = ApplicationSlice::new(name.to_owned());
    slice
        .add_collection(Collection::new::<i32>("numbers"))
        .unwrap();
    slice
        .add_collection(Collection::new::<String>("labels"))
        .unwrap();
    slice
}

#[test]
fn resolves_names_to_keys_and_indexed_collections() {
    let mut application = Application::default();
    let id = application.add_slice(slice("main")).unwrap();
    let key = application.collection_key("main", "labels").unwrap();
    assert_eq!(key, CollectionKey::new(id, 1));
    assert_eq!(application.slice_index("main").unwrap(), 0);
    assert_eq!(
        application
            .slice(id)
            .unwrap()
            .collection_index("labels")
            .unwrap(),
        1
    );
    let collection = application.resolve_collection(key).unwrap();
    assert_eq!(collection.name(), "labels");
    assert!(collection.arena::<String>().is_some());
    assert!(collection.arena::<i32>().is_none());
    assert!(application.collection_key("Main", "labels").is_err());
    assert!(application.collection_key("main", "Labels").is_err());
    assert!(
        application
            .resolve_collection(CollectionKey::new(id, usize::MAX))
            .is_err()
    );
    let foreign = ApplicationSlice::new("foreign").id();
    assert!(
        application
            .resolve_collection(CollectionKey::new(foreign, 0))
            .is_err()
    );
}

#[test]
fn maintains_name_maps_on_reordering_removal_and_name_reuse() {
    let mut application = Application::default();
    let first = application.add_slice(slice("first")).unwrap();
    let middle = application.add_slice(slice("middle")).unwrap();
    let last = application.add_slice(slice("last")).unwrap();
    let first_key = application.collection_key("first", "numbers").unwrap();
    let middle_key = application.collection_key("middle", "numbers").unwrap();
    let last_key = application.collection_key("last", "numbers").unwrap();
    application.swap_slices(first, last).unwrap();
    assert_eq!(application.slice_index("last").unwrap(), 0);
    assert_eq!(application.slice_index("first").unwrap(), 2);
    assert_eq!(application.slice_named("first").unwrap().id(), first);
    assert_eq!(
        application.resolve_collection(first_key).unwrap().name(),
        "numbers"
    );
    application.remove_slice(middle).unwrap();
    assert!(application.slice_index("middle").is_err());
    assert!(application.resolve_collection(middle_key).is_err());
    assert_eq!(application.slice_index("first").unwrap(), 1);
    assert_eq!(
        application.collection_key("last", "numbers").unwrap(),
        last_key
    );
    application
        .add_collection(first, Collection::new::<bool>("flags"))
        .unwrap();
    assert_eq!(
        application
            .slice(first)
            .unwrap()
            .collection_index("flags")
            .unwrap(),
        2
    );
    assert_eq!(
        application.collection_key("first", "numbers").unwrap(),
        first_key
    );
    let replacement = application.add_slice(slice("middle")).unwrap();
    assert_ne!(replacement, middle);
    assert!(application.resolve_collection(middle_key).is_err());
    assert_eq!(
        application
            .collection_key("middle", "numbers")
            .unwrap()
            .slice,
        replacement
    );
    assert!(application.swap_slices(first, middle).is_err());
    assert_eq!(application.slice_index("first").unwrap(), 1);
}

#[test]
fn rejected_duplicate_and_empty_names_do_not_change_lookup_maps() {
    let mut application = Application::default();
    let id = application.add_slice(slice("main")).unwrap();
    assert!(application.add_slice(slice("main")).is_err());
    assert!(application.add_slice(slice("")).is_err());
    assert!(
        application
            .add_collection(id, Collection::new::<bool>("numbers"))
            .is_err()
    );
    assert!(
        application
            .add_collection(id, Collection::new::<bool>(""))
            .is_err()
    );
    assert_eq!(application.slices().len(), 1);
    assert_eq!(
        application.collection_key("main", "numbers").unwrap(),
        CollectionKey::new(id, 0)
    );
    assert_eq!(application.slice(id).unwrap().collections().len(), 2);
    assert!(application.collection_key("main", "").is_err());
}

#[test]
fn worker_handle_returns_a_key_for_later_resolution() {
    let handle = Application::new();
    let id = handle.add_slice(slice("main")).unwrap();
    let key = handle.collection_key("main", "labels").unwrap();
    assert_eq!(key, CollectionKey::new(id, 1));
    assert_eq!(
        handle
            .inspect(move |state| Ok(state.resolve_collection(key)?.name().to_owned()))
            .unwrap(),
        "labels"
    );
    assert!(handle.collection_key("missing", "labels").is_err());
}
