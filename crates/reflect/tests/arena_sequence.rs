use pixui_base::Arena;
use pixui_reflect::{DynamicObject, Reflect};

#[test]
fn arena_views_have_dense_indices_and_reflect_live_values_in_slot_order() {
    let mut arena = Arena::new();
    let first = arena.insert(String::from("first"));
    let removed = arena.insert(String::from("removed"));
    let last = arena.insert(String::from("last"));
    arena.remove(removed);
    let view = DynamicObject::from_arena(&arena);
    assert_eq!(view.len().unwrap(), 2);
    assert_eq!(
        view.get(0).unwrap().downcast_ref::<String>().unwrap(),
        "first"
    );
    let item = view.get(1).unwrap();
    assert!(std::ptr::eq(
        item.downcast_ref::<String>().unwrap(),
        arena.get(last).unwrap()
    ));
    assert!(view.get(2).is_err());
    drop(item);
    drop(view);
    arena.insert(String::from("replacement"));
    let mut view = DynamicObject::from_arena(&arena);
    assert_eq!(view.len().unwrap(), 3);
    assert_eq!(
        view.get(1).unwrap().downcast_ref::<String>().unwrap(),
        "replacement"
    );
    assert!(std::ptr::eq(
        view.descriptor().element_type().unwrap(),
        String::type_descriptor()
    ));
    assert!(view.get_mut(0).is_err());
    drop(view);
    arena.remove(first);
    arena.clear();
    let view = DynamicObject::from_arena(&arena);
    assert!(view.is_empty().unwrap());
    assert!(view.get(0).is_err());
}
