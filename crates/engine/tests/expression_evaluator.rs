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
    let mut app = Application::default();
    let id = app.add_slice(ApplicationSlice::new("data")).unwrap();
    let labels = app
        .add_collection(id, Collection::new_reflected::<String>("labels"))
        .unwrap();
    let items = app
        .add_collection(id, Collection::new_reflected::<model::Item>("items"))
        .unwrap();
    let arena = app.resolve_collection_mut::<model::Item>(items).unwrap();
    let removed = arena.insert(model::Item { value: 1 });
    let key = arena.insert(model::Item { value: 42 });
    arena.remove(removed);
    let mut sequence: DynamicObject<'_> = {
        let context = ExpressionContext::new(&app);
        evaluate(&context, &Expression::collection(items)).unwrap()
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
        app.collection(id, "items")
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
        &Expression::collection(labels),
    )
    .unwrap();
    assert!(empty.is_empty().unwrap());
    assert!(empty.get(0).is_err());
}

#[test]
fn rejects_foreign_addresses_and_non_reflected_collections() {
    let mut app = Application::default();
    let plain = app
        .register_collection(Collection::new::<i32>("plain"))
        .unwrap();
    let foreign = Application::default()
        .register_collection(Collection::new_reflected::<i32>("foreign"))
        .unwrap();
    for index in [plain, foreign] {
        assert!(
            evaluate(
                &ExpressionContext::new(&app),
                &Expression::collection(index)
            )
            .is_err()
        );
    }
}

#[test]
fn addresses_survive_slice_removal_reordering_and_collection_append() {
    let mut app = Application::default();
    let first = app.add_slice(ApplicationSlice::new("first")).unwrap();
    let index = app
        .add_collection(first, Collection::new_reflected::<i32>("original"))
        .unwrap();
    app.resolve_collection_mut::<i32>(index).unwrap().insert(42);
    let second = app.add_slice(ApplicationSlice::new("second")).unwrap();
    let expression = Expression::collection(index);
    app.add_collection(first, Collection::new::<i32>("appended"))
        .unwrap();
    app.swap_slices(first, second).unwrap();
    app.remove_slice(first).unwrap();
    let collection = evaluate(&ExpressionContext::new(&app), &expression).unwrap();
    assert_eq!(
        *collection.get(0).unwrap().downcast_ref::<i32>().unwrap(),
        42
    );
}

#[test]
fn worker_inspection_returns_an_owned_snapshot_of_the_sequence() {
    let handle = Application::new();
    let mut numbers = Collection::new_reflected::<i32>("numbers");
    numbers.arena_mut::<i32>().unwrap().insert(7);
    let index = handle.register_collection(numbers).unwrap();
    let values = handle
        .inspect(move |state| {
            let sequence = evaluate(
                &ExpressionContext::new(state),
                &Expression::collection(index),
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
