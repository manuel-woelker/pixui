use std::cell::Cell;

use pixui_reflect::{Reflect, TypeDescriptor};

#[pixui_reflect::reflect(send)]
mod model {
    use super::Cell;
    pub struct Inputs {
        pub title: String,
        pub value: Cell<i32>,
        #[cfg(any())]
        disabled: bool,
    }
    pub struct Empty;
}

#[test]
fn constructs_send_values_without_requiring_sync_and_moves_them_to_a_thread() {
    let descriptor = model::Inputs::type_descriptor();
    assert!(descriptor.is_constructible());
    assert!(descriptor.is_send_constructible());
    let value = descriptor
        .construct_send(vec![
            Box::new(String::from("input")),
            Box::new(Cell::new(7)),
        ])
        .unwrap();
    let returned = std::thread::spawn(move || {
        let value = value.downcast::<model::Inputs>().unwrap();
        value.value.set(8);
        value
    })
    .join()
    .unwrap();
    assert_eq!(returned.title, "input");
    assert_eq!(returned.value.get(), 8);
    assert!(
        model::Empty::type_descriptor()
            .construct_send(vec![])
            .unwrap()
            .is::<model::Empty>()
    );
}

#[test]
fn validates_sendable_field_counts_types_and_custom_constructor_results() {
    let descriptor = model::Inputs::type_descriptor();
    assert!(descriptor.construct_send(vec![]).is_err());
    assert!(
        descriptor
            .construct_send(vec![
                Box::new(String::new()),
                Box::new(Cell::new(0)),
                Box::new(false)
            ])
            .is_err()
    );
    let error = descriptor
        .construct_send(vec![Box::new(String::new()), Box::new(3i32)])
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("field 1 `value`"));
    assert!(error.contains("Cell"));
    assert!(!String::type_descriptor().is_send_constructible());
    assert!(String::type_descriptor().construct_send(vec![]).is_err());
    let wrong = TypeDescriptor::new::<bool>(vec![], vec![])
        .unwrap()
        .with_send_constructor(|_| Ok(Box::new(1i32)));
    assert!(wrong.construct_send(vec![]).is_err());
}
