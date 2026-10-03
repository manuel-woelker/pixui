use std::any::TypeId;

use pixui_engine::application::{
    app::Application, application_slice::ApplicationSlice, collection::Collection,
};

#[test]
fn slices_hold_collections_with_different_homogeneous_item_types() {
    struct Record {
        value: i32,
    }
    let mut records = Collection::new::<Record>("records");
    let record_key = records
        .arena_mut::<Record>()
        .unwrap()
        .insert(Record { value: 7 });
    let mut labels = Collection::new::<String>(String::from("labels"));
    let label_key = labels.arena_mut::<String>().unwrap().insert("hello".into());

    let mut slice = ApplicationSlice::new("main");
    slice.collections.extend([records, labels]);
    let mut application = Application::new();
    application.slices.push(slice);

    let collections = &application.slices[0].collections;
    assert_eq!(application.slices[0].name, "main");
    assert_eq!(collections[0].name, "records");
    assert_eq!(
        collections[0]
            .arena::<Record>()
            .unwrap()
            .get(record_key)
            .unwrap()
            .value,
        7
    );
    assert_eq!(
        collections[1]
            .arena::<String>()
            .unwrap()
            .get(label_key)
            .unwrap(),
        "hello"
    );
    assert_eq!(collections[0].item_type_id(), TypeId::of::<Record>());
    assert_eq!(
        collections[1].item_type_name(),
        std::any::type_name::<String>()
    );
}

#[test]
fn wrong_type_access_is_rejected_and_typed_keys_keep_arena_checks() {
    let mut first = Collection::new::<i32>("first");
    let mut second = Collection::new::<i32>("second");
    let key = first.arena_mut::<i32>().unwrap().insert(1);
    assert!(first.arena::<String>().is_none());
    assert!(first.arena_mut::<String>().is_none());
    assert!(second.arena::<i32>().unwrap().get(key).is_none());
    *first.arena_mut::<i32>().unwrap().get_mut(key).unwrap() = 2;
    assert_eq!(first.arena_mut::<i32>().unwrap().remove(key), Some(2));
    assert!(first.arena::<i32>().unwrap().get(key).is_none());
    assert!(second.arena_mut::<i32>().unwrap().is_empty());
}
