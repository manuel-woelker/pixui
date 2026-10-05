use pixui_engine::{
    application::{app::Application, application_slice::ApplicationSlice, collection::Collection},
    expression::{context::ExpressionContext, evaluator::evaluate, expression::Expression},
};
use pixui_reflect::{DynamicObject, Reflect};

#[pixui_reflect::reflect]
mod model {
    pub struct Item {
        pub value: i32,
    }
}

#[test]
fn evaluates_as_a_shared_sequence_borrowing_live_items_and_skipping_holes() {
    let mut slice = ApplicationSlice::new("data");
    slice
        .add_collection(Collection::new_reflected::<String>("labels"))
        .unwrap();
    slice
        .add_collection(Collection::new_reflected::<model::Item>("items"))
        .unwrap();
    let arena = slice.collection_mut::<model::Item>("items").unwrap();
    let removed = arena.insert(model::Item { value: 1 });
    let key = arena.insert(model::Item { value: 42 });
    arena.remove(removed);
    let mut app = Application::default();
    let id = app.add_slice(slice).unwrap();
    let mut sequence: DynamicObject<'_> = {
        let context = ExpressionContext::new(&app);
        evaluate(&context, &Expression::collection(id, 1)).unwrap()
    };
    assert_eq!(sequence.len().unwrap(), 1);
    assert!(sequence.descriptor().is_sequence());
    assert!(std::ptr::eq(
        sequence.descriptor().element_type().unwrap(),
        model::Item::type_descriptor()
    ));
    let item = sequence.get(0).unwrap();
    assert_eq!(
        *item
            .read_named("value")
            .unwrap()
            .downcast_ref::<i32>()
            .unwrap(),
        42
    );
    assert!(std::ptr::eq(
        item.downcast_ref::<model::Item>().unwrap(),
        app.slice(id)
            .unwrap()
            .collection("items")
            .unwrap()
            .arena::<model::Item>()
            .unwrap()
            .get(key)
            .unwrap()
    ));
    assert!(!sequence.is_mutable());
    assert!(!item.is_mutable());
    drop(item);
    assert!(sequence.get(1).is_err());
    assert!(sequence.get(usize::MAX).is_err());
    assert!(sequence.get_mut(0).is_err());
    assert!(sequence.into_owned::<Vec<model::Item>>().is_err());
    let empty = evaluate(
        &ExpressionContext::new(&app),
        &Expression::collection(id, 0),
    )
    .unwrap();
    assert!(empty.is_empty().unwrap());
    assert!(empty.get(0).is_err());
}

#[test]
fn rejects_invalid_addresses_and_non_reflected_collections() {
    let mut app = Application::default();
    let empty = app.add_slice(ApplicationSlice::new("empty")).unwrap();
    let mut slice = ApplicationSlice::new("values");
    slice
        .add_collection(Collection::new::<i32>("plain"))
        .unwrap();
    let id = app.add_slice(slice).unwrap();
    let foreign = ApplicationSlice::new("foreign").id();
    for (slice, index) in [(empty, 0), (id, 0), (id, 1), (id, usize::MAX), (foreign, 0)] {
        assert!(
            evaluate(
                &ExpressionContext::new(&app),
                &Expression::collection(slice, index)
            )
            .is_err()
        );
    }
    app.remove_slice(id).unwrap();
    assert!(
        evaluate(
            &ExpressionContext::new(&app),
            &Expression::collection(id, 0)
        )
        .is_err()
    );
}

#[test]
fn addresses_survive_slice_reordering_and_collection_append() {
    let mut app = Application::default();
    let mut first = ApplicationSlice::new("first");
    first
        .add_collection(Collection::new_reflected::<i32>("original"))
        .unwrap();
    first.collection_mut::<i32>("original").unwrap().insert(42);
    let first = app.add_slice(first).unwrap();
    let second = app.add_slice(ApplicationSlice::new("second")).unwrap();
    let expression = Expression::collection(first, 0);
    app.add_collection(first, Collection::new::<i32>("appended"))
        .unwrap();
    app.swap_slices(first, second).unwrap();
    let collection = evaluate(&ExpressionContext::new(&app), &expression).unwrap();
    assert_eq!(
        *collection.get(0).unwrap().downcast_ref::<i32>().unwrap(),
        42
    );
}

#[test]
fn worker_inspection_returns_an_owned_snapshot_of_the_sequence() {
    let handle = Application::new();
    let mut slice = ApplicationSlice::new("data");
    slice
        .add_collection(Collection::new_reflected::<i32>("numbers"))
        .unwrap();
    slice.collection_mut::<i32>("numbers").unwrap().insert(7);
    let id = handle.add_slice(slice).unwrap();
    let values = handle
        .inspect(move |state| {
            let sequence = evaluate(
                &ExpressionContext::new(state),
                &Expression::collection(id, 0),
            )?;
            (0..sequence.len()?)
                .map(|index| Ok(*sequence.get(index)?.downcast_ref::<i32>().unwrap()))
                .collect::<pixui_base::PixuiResult<Vec<_>>>()
        })
        .unwrap();
    assert_eq!(values, [7]);
}

#[test]
fn field_expressions_read_the_current_value_and_validate_context() {
    #[pixui_reflect::reflect]
    mod fields {
        pub struct Root {
            pub values: Vec<i32>,
            pub scalar: i32,
        }
    }
    let root = DynamicObject::from_reflect(fields::Root {
        values: vec![4, 5],
        scalar: 7,
    });
    let expression = Expression::field(
        fields::Root::type_descriptor()
            .field_index("values")
            .unwrap(),
    );
    let sequence = evaluate(&ExpressionContext::from_value(&root), &expression).unwrap();
    assert_eq!(sequence.len().unwrap(), 2);
    assert_eq!(*sequence.get(1).unwrap().downcast_ref::<i32>().unwrap(), 5);
    assert!(
        evaluate(
            &ExpressionContext::from_value(&root),
            &Expression::field(pixui_reflect::FieldIndex(99))
        )
        .is_err()
    );
    let application = Application::default();
    assert!(evaluate(&ExpressionContext::new(&application), &expression).is_err());
    let scalar = evaluate(
        &ExpressionContext::from_value(&root),
        &Expression::field(
            fields::Root::type_descriptor()
                .field_index("scalar")
                .unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(scalar.downcast_ref::<i32>(), Some(&7));
}
