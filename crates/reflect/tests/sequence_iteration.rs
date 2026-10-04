use pixui_base::Arena;
use pixui_reflect::DynamicObject;

fn values(object: &DynamicObject<'_>) -> Vec<i32> {
    object
        .iter()
        .unwrap()
        .map(|item| {
            assert!(!item.is_mutable());
            *item.downcast_ref::<i32>().unwrap()
        })
        .collect()
}

#[test]
fn iterates_owned_shared_and_mutable_vectors_and_slices_in_order() {
    let mut vector = vec![3, 1, 2];
    assert_eq!(
        values(&DynamicObject::from_reflect(vector.clone())),
        [3, 1, 2]
    );
    assert_eq!(values(&DynamicObject::from_ref(&vector)), [3, 1, 2]);
    assert_eq!(values(&DynamicObject::from_mut(&mut vector)), [3, 1, 2]);
    assert_eq!(values(&DynamicObject::from_slice(&vector)), [3, 1, 2]);
    assert_eq!(
        values(&DynamicObject::from_slice_mut(&mut vector)),
        [3, 1, 2]
    );
    assert!(values(&DynamicObject::from_reflect(Vec::<i32>::new())).is_empty());
    assert!(values(&DynamicObject::from_slice::<i32>(&[])).is_empty());
}

#[test]
fn arena_iteration_skips_holes_and_borrows_original_elements_without_cloning() {
    let mut arena = Arena::new();
    let removed = arena.insert(10);
    let first = arena.insert(20);
    let last = arena.insert(30);
    arena.remove(removed);
    let view = DynamicObject::from_arena(&arena);
    assert_eq!(values(&view), [20, 30]);
    let mut iter = view.iter().unwrap();
    let first_item = iter.next().unwrap();
    let last_item = iter.next().unwrap();
    assert!(std::ptr::eq(
        first_item.downcast_ref::<i32>().unwrap(),
        arena.get(first).unwrap()
    ));
    assert!(std::ptr::eq(
        last_item.downcast_ref::<i32>().unwrap(),
        arena.get(last).unwrap()
    ));
    assert!(iter.next().is_none());
    assert!(iter.next().is_none());
    // Element views borrow the source, rather than the iterator allocation.
    drop(iter);
    assert_eq!(*first_item.downcast_ref::<i32>().unwrap(), 20);
    assert!(
        DynamicObject::from_arena(&Arena::<i32>::new())
            .iter()
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn iteration_preserves_reflection_and_rejects_non_sequences() {
    #[pixui_reflect::reflect]
    mod model {
        pub struct Item {
            pub value: i32,
        }
    }
    let items = vec![model::Item { value: 42 }];
    let view = DynamicObject::from_slice(&items);
    let item = view.iter().unwrap().next().unwrap();
    assert_eq!(
        *item
            .read_named("value")
            .unwrap()
            .downcast_ref::<i32>()
            .unwrap(),
        42
    );
    assert!(DynamicObject::from_reflect(42i32).iter().is_err());
    assert!(
        DynamicObject::from_reflect(model::Item { value: 1 })
            .iter()
            .is_err()
    );
}
