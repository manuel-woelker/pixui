use pixui_reflect::{DynamicObject, Reflect, TypeDescriptor};

#[pixui_reflect::reflect]
mod model {
    pub struct Arguments {
        pub title: String,
        pub done: bool,
        #[cfg(any())]
        unavailable: i32,
    }
    pub struct Empty {}
    pub struct Marker;
    pub struct MoveOnly {
        pub value: std::sync::Mutex<i32>,
    }
}

#[test]
fn constructs_in_field_order_and_supports_empty_structs() {
    let descriptor = model::Arguments::type_descriptor();
    assert!(descriptor.is_constructible());
    assert_eq!(
        descriptor.fields()[0].type_id(),
        Some(std::any::TypeId::of::<String>())
    );
    assert_eq!(descriptor.fields()[1].type_name(), Some("bool"));
    assert_eq!(
        descriptor
            .fields()
            .iter()
            .map(|f| f.name)
            .collect::<Vec<_>>(),
        ["title", "done"]
    );
    let object = descriptor
        .construct(vec![
            DynamicObject::from_reflect(String::from("Buy milk")),
            DynamicObject::from_reflect(true),
        ])
        .unwrap();
    assert!(object.is_owned());
    let args = object.into_owned::<model::Arguments>().unwrap();
    assert_eq!(args.title, "Buy milk");
    assert!(args.done);
    assert!(
        model::Empty::type_descriptor()
            .construct(vec![])
            .unwrap()
            .downcast_ref::<model::Empty>()
            .is_some()
    );
    assert!(
        model::Marker::type_descriptor()
            .construct(vec![])
            .unwrap()
            .downcast_ref::<model::Marker>()
            .is_some()
    );
}

#[test]
fn rejects_missing_extra_wrong_and_borrowed_values() {
    let descriptor = model::Arguments::type_descriptor();
    for fields in [
        vec![],
        vec![DynamicObject::from_reflect(String::new())],
        vec![
            DynamicObject::from_reflect(String::new()),
            DynamicObject::from_reflect(false),
            DynamicObject::from_reflect(1i32),
        ],
    ] {
        assert!(
            descriptor
                .construct(fields)
                .err()
                .unwrap()
                .to_string()
                .contains("expects 2 fields")
        );
    }
    let error = descriptor
        .construct(vec![
            DynamicObject::from_reflect(String::new()),
            DynamicObject::from_reflect(7i32),
        ])
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("field 1 `done`"));
    assert!(error.contains("bool"));
    static DONE: bool = false;
    let error = descriptor
        .construct(vec![
            DynamicObject::from_reflect(String::new()),
            DynamicObject::from_ref(&DONE),
        ])
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("owned"));
    assert!(
        descriptor
            .construct(vec![
                DynamicObject::from_reflect(false),
                DynamicObject::from_reflect(String::new()),
            ])
            .is_err()
    );
}

#[test]
fn moves_nonclone_values_and_rejects_unregistered_construction() {
    let descriptor = Box::leak(Box::new(
        TypeDescriptor::new::<std::sync::Mutex<i32>>(vec![], vec![]).unwrap(),
    ));
    let field = DynamicObject::new(std::sync::Mutex::new(42), descriptor).unwrap();
    let value = model::MoveOnly::type_descriptor()
        .construct(vec![field])
        .unwrap()
        .into_owned::<model::MoveOnly>()
        .unwrap();
    assert_eq!(*value.value.lock().unwrap(), 42);
    assert!(!String::type_descriptor().is_constructible());
    assert!(String::type_descriptor().construct(vec![]).is_err());
    assert!(Vec::<String>::type_descriptor().construct(vec![]).is_err());
}

#[test]
fn custom_constructor_must_return_the_registered_owned_type() {
    let wrong = TypeDescriptor::new::<bool>(vec![], vec![])
        .unwrap()
        .with_constructor(|_| Ok(DynamicObject::from_reflect(1i32)));
    assert!(wrong.construct(vec![]).is_err());
    let borrowed = TypeDescriptor::new::<bool>(vec![], vec![])
        .unwrap()
        .with_constructor(|_| {
            static VALUE: bool = true;
            Ok(DynamicObject::from_ref(&VALUE))
        });
    assert!(borrowed.construct(vec![]).is_err());
}
