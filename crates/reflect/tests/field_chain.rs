//! Consuming borrowed views preserve receiver lifetimes across nested fields.
use pixui_reflect::{DynamicObject, Reflect};
#[pixui_reflect::reflect]
mod data {
    pub struct Inner {
        pub value: i32,
    }
    pub struct Outer {
        pub inner: Inner,
    }
}
#[test]
fn typed_then_reflected_field_chain_preserves_borrow() {
    let outer = data::Outer {
        inner: data::Inner { value: 42 },
    };
    let view = DynamicObject::from_ref(&outer);
    let inner = view
        .shared_view()
        .unwrap()
        .into_read_with_descriptor(
            view.field_index("inner").unwrap(),
            data::Inner::type_descriptor(),
        )
        .unwrap();
    let value = inner
        .into_read_object(data::Inner::type_descriptor().field_index("value").unwrap())
        .unwrap();
    assert_eq!(value.downcast_ref::<i32>(), Some(&42));
}
#[test]
fn chains_reject_owned_storage_wrong_descriptors_and_slice_views() {
    let outer = data::Outer {
        inner: data::Inner { value: 42 },
    };
    let index = data::Outer::type_descriptor().field_index("inner").unwrap();
    assert!(
        DynamicObject::from_ref(&outer)
            .into_read_with_descriptor(index, String::type_descriptor())
            .is_err()
    );
    assert!(
        DynamicObject::from_reflect(outer)
            .into_read_with_descriptor(index, data::Inner::type_descriptor())
            .is_err()
    );
    assert!(DynamicObject::from_slice(&[1i32, 2]).shared_view().is_err());
}
