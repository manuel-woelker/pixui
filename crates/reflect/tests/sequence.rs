use pixui_reflect::{DynamicObject, Reflect, TypeKind};

#[pixui_reflect::reflect]
mod model {
    pub struct Item {
        pub value: i32,
    }
    impl Item {
        pub fn add(&mut self, amount: i32) {
            self.value += amount;
        }
    }
    pub struct Collection {
        pub items: Vec<Item>,
    }
    impl Collection {
        pub fn items(&self) -> &[Item] {
            &self.items
        }
        pub fn items_mut(&mut self) -> &mut [Item] {
            &mut self.items
        }
        pub fn vector(&self) -> &Vec<Item> {
            &self.items
        }
    }
    pub struct Other;
    pub struct Node {
        pub children: Vec<Node>,
    }
}

fn items() -> Vec<model::Item> {
    vec![model::Item { value: 1 }, model::Item { value: 2 }]
}
fn value(object: &DynamicObject<'_>) -> i32 {
    *object
        .read_named("value")
        .unwrap()
        .downcast_ref::<i32>()
        .unwrap()
}

#[test]
fn type_kind_and_element_metadata_are_independent_of_storage() {
    assert!(matches!(
        model::Item::type_descriptor().kind(),
        TypeKind::Struct
    ));
    assert!(model::Item::type_descriptor().element_type().is_none());
    let owned = DynamicObject::from_reflect(items());
    let values = items();
    let shared = DynamicObject::from_ref(&values);
    let slice = DynamicObject::from_slice(&values);
    for object in [&owned, &shared, &slice] {
        assert!(matches!(object.descriptor().kind(), TypeKind::Sequence(_)));
        assert!(object.descriptor().is_sequence());
        assert!(std::ptr::eq(
            object.descriptor().element_type().unwrap(),
            model::Item::type_descriptor()
        ));
    }
    assert!(std::ptr::eq(owned.descriptor(), shared.descriptor()));
    assert_ne!(owned.descriptor().type_id(), slice.descriptor().type_id());
    assert_eq!(
        slice.descriptor().type_id(),
        std::any::TypeId::of::<[model::Item]>()
    );
    assert!(owned.downcast_ref::<Vec<model::Item>>().is_some());
    assert!(slice.downcast_ref::<Vec<model::Item>>().is_none());
}

#[test]
fn owned_vectors_allow_reflected_element_access_and_mutation() {
    let mut object: DynamicObject<'static> = DynamicObject::from_reflect(items());
    assert_eq!(object.len().unwrap(), 2);
    assert!(!object.is_empty().unwrap());
    let element = object.get(0).unwrap();
    assert!(!element.is_owned());
    assert!(!element.is_mutable());
    assert_eq!(value(&element), 1);
    drop(element);
    object
        .get_mut(1)
        .unwrap()
        .invoke_named("add", &[&10_i32])
        .unwrap();
    assert_eq!(value(&object.get(1).unwrap()), 12);
    // Sequence access does not prevent ordinary vector access after the borrow.
    object
        .downcast_mut::<Vec<model::Item>>()
        .unwrap()
        .push(model::Item { value: 3 });
    assert_eq!(object.len().unwrap(), 3);
}

#[test]
fn vector_views_respect_write_capability() {
    let mut values = items();
    {
        let mut shared = DynamicObject::from_ref(&values);
        assert!(!shared.is_mutable());
        assert!(shared.get_mut(0).is_err());
        assert_eq!(value(&shared.get(0).unwrap()), 1);
    }
    {
        let mut mutable = DynamicObject::from_mut(&mut values);
        assert!(mutable.is_mutable());
        mutable
            .get_mut(0)
            .unwrap()
            .downcast_mut::<model::Item>()
            .unwrap()
            .value = 7;
    }
    assert_eq!(values[0].value, 7);
}

#[test]
fn slice_views_borrow_original_elements_and_reject_struct_dispatch() {
    let mut values = items();
    {
        let mut shared = DynamicObject::from_slice(&values[1..]);
        assert_eq!(shared.len().unwrap(), 1);
        assert!(!shared.is_owned());
        assert!(!shared.is_mutable());
        assert!(std::ptr::eq(
            shared
                .get(0)
                .unwrap()
                .downcast_ref::<model::Item>()
                .unwrap(),
            &values[1]
        ));
        assert!(shared.get_mut(0).is_err());
        assert!(shared.read(pixui_reflect::FieldIndex(0)).is_err());
        assert!(shared.invoke(pixui_reflect::MethodIndex(0), &[]).is_err());
        assert!(shared.downcast_mut::<Vec<model::Item>>().is_none());
    }
    {
        let mut mutable = DynamicObject::from_slice_mut(&mut values[1..]);
        assert!(mutable.is_mutable());
        mutable
            .get_mut(0)
            .unwrap()
            .invoke_named("add", &[&4_i32])
            .unwrap();
        assert_eq!(value(&mutable.get(0).unwrap()), 6);
    }
    assert_eq!(values[1].value, 6);
    assert_eq!(values[0].value, 1);
}

#[test]
fn bounds_empty_and_non_sequence_access_return_errors() {
    let mut vector = DynamicObject::from_reflect(Vec::<model::Item>::new());
    assert_eq!(vector.len().unwrap(), 0);
    assert!(vector.is_empty().unwrap());
    assert!(vector.get(0).is_err());
    assert!(vector.get_mut(0).is_err());
    let mut values = items();
    let mut slice = DynamicObject::from_slice_mut(&mut values);
    for index in [2, usize::MAX] {
        assert!(slice.get(index).is_err());
        assert!(slice.get_mut(index).is_err());
    }
    let empty = DynamicObject::from_slice::<model::Item>(&[]);
    assert!(empty.is_empty().unwrap());
    assert!(empty.get(0).is_err());
    let mut item = DynamicObject::from_reflect(model::Item { value: 0 });
    assert!(item.len().is_err());
    assert!(item.is_empty().is_err());
    assert!(item.get(0).is_err());
    assert!(item.get_mut(0).is_err());
}

#[test]
fn nested_and_recursive_sequence_descriptors_are_lazy() {
    let nested = DynamicObject::from_reflect(vec![items()]);
    let inner = nested.get(0).unwrap();
    assert!(inner.descriptor().is_sequence());
    assert_eq!(value(&inner.get(1).unwrap()), 2);
    let descriptor = Vec::<model::Node>::type_descriptor();
    assert!(std::ptr::eq(
        descriptor.element_type().unwrap(),
        model::Node::type_descriptor()
    ));
    let nodes = DynamicObject::from_reflect(vec![model::Node { children: vec![] }]);
    assert!(
        nodes
            .get(0)
            .unwrap()
            .read_named("children")
            .unwrap()
            .downcast_ref::<Vec<model::Node>>()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn generic_descriptors_are_cached_per_type_across_threads() {
    let descriptors: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(Vec::<model::Item>::type_descriptor))
        .collect();
    let expected = Vec::<model::Item>::type_descriptor();
    for descriptor in descriptors {
        assert!(std::ptr::eq(descriptor.join().unwrap(), expected));
    }
    assert!(!std::ptr::eq(
        expected,
        Vec::<model::Other>::type_descriptor()
    ));
    assert!(std::ptr::eq(
        Vec::<model::Other>::type_descriptor()
            .element_type()
            .unwrap(),
        model::Other::type_descriptor()
    ));
}

#[test]
fn ordinary_methods_return_reflected_vectors_and_slices() {
    let mut object = DynamicObject::from_reflect(model::Collection { items: items() });
    {
        let slice = object.invoke_ref_named("items", &[]).unwrap();
        assert_eq!(slice.len().unwrap(), 2);
        assert_eq!(value(&slice.get(0).unwrap()), 1);
        let vector = object.invoke_ref_named("vector", &[]).unwrap();
        assert_eq!(vector.len().unwrap(), 2);
        assert!(vector.downcast_ref::<Vec<model::Item>>().is_some());
    }
    {
        let mut slice = object.invoke_mut_named("items_mut", &[]).unwrap();
        slice
            .get_mut(0)
            .unwrap()
            .invoke_named("add", &[&10_i32])
            .unwrap();
    }
    assert_eq!(
        object.downcast_ref::<model::Collection>().unwrap().items[0].value,
        11
    );
}

#[test]
fn vector_fields_can_be_read_as_reflected_sequences() {
    let object = DynamicObject::from_reflect(model::Collection { items: items() });
    let index = object.field_index("items").unwrap();
    let sequence = object.read_object(index).unwrap();
    assert!(sequence.descriptor().is_sequence());
    assert_eq!(sequence.len().unwrap(), 2);
    assert_eq!(value(&sequence.get(1).unwrap()), 2);
    let original = &object.downcast_ref::<model::Collection>().unwrap().items;
    assert!(std::ptr::eq(
        sequence.downcast_ref::<Vec<model::Item>>().unwrap(),
        original
    ));
    assert_eq!(object.read_object_named("items").unwrap().len().unwrap(), 2);
    assert!(object.read_object(pixui_reflect::FieldIndex(99)).is_err());
    let item = DynamicObject::from_reflect(model::Item { value: 0 });
    assert!(item.read_object_named("value").is_err());
    assert!(item.read_object_named("missing").is_err());
}
