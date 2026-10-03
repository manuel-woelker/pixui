use std::any::Any;

use pixui_base::message;
use pixui_reflection::{Field, FieldIndex, Method, MethodIndex, TypeDescriptor, argument};

#[derive(Default)]
struct Counter {
    name: String,
    value: i32,
}

fn reflection() -> TypeDescriptor {
    pixui_reflection::type_descriptor!(Counter,
        fields: [name, value],
        methods: [
            "add" => (1, |counter, args| {
                let amount = *argument::<i32>(args, 0)?;
                counter.value += amount;
                Ok(Box::new(counter.value))
            }),
            "reset" => (0, |counter, _| {
                counter.value = 0;
                Ok(Box::new(()))
            }),
            "fail" => (0, |_, _| Err(message("adapter failed"))),
            "sum" => (2, |_, args| {
                Ok(Box::new(*argument::<i32>(args, 0)? + *argument::<i32>(args, 1)?))
            }),
        ]
    )
    .unwrap()
}

#[test]
fn lookup_and_metadata_follow_registration_order() {
    let descriptor = reflection();
    assert_eq!(descriptor.field_index("name").unwrap(), FieldIndex(0));
    assert_eq!(descriptor.field_index("value").unwrap(), FieldIndex(1));
    assert_eq!(descriptor.method_index("add").unwrap(), MethodIndex(0));
    assert_eq!(descriptor.method_index("sum").unwrap(), MethodIndex(3));
    assert_eq!(
        descriptor
            .fields()
            .iter()
            .map(|f| f.name)
            .collect::<Vec<_>>(),
        ["name", "value"]
    );
    assert_eq!(descriptor.methods()[3].arity, 2);
}

#[test]
fn indexed_and_named_reads_borrow_original_fields() {
    let descriptor = reflection();
    let counter = Counter {
        name: "hello".into(),
        value: 7,
    };
    let name = descriptor
        .read(&counter, descriptor.field_index("name").unwrap())
        .unwrap()
        .downcast_ref::<String>()
        .unwrap();
    assert!(std::ptr::eq(name, &counter.name));
    let value = descriptor.read_named(&counter, "value").unwrap();
    assert_eq!(value.downcast_ref::<i32>(), Some(&7));
    assert!(value.downcast_ref::<String>().is_none());
}

#[test]
fn cached_indices_work_across_receivers_and_mutations() {
    let descriptor = reflection();
    let add = descriptor.method_index("add").unwrap();
    let value = descriptor.field_index("value").unwrap();
    let mut first = Counter::default();
    let mut second = Counter::default();
    for amount in [2, 3] {
        let result = descriptor.invoke(&mut first, add, &[&amount]).unwrap();
        assert_eq!(*result.downcast::<i32>().unwrap(), first.value);
    }
    descriptor.invoke(&mut second, add, &[&9_i32]).unwrap();
    assert_eq!(
        descriptor
            .read(&first, value)
            .unwrap()
            .downcast_ref::<i32>(),
        Some(&5)
    );
    assert_eq!(second.value, 9);
}

#[test]
fn named_invocation_handles_multiple_arguments_and_unit_results() {
    let mut counter = Counter {
        value: 42,
        ..Counter::default()
    };
    let descriptor = reflection();
    let result = descriptor
        .invoke_named(&mut counter, "sum", &[&2_i32, &3_i32])
        .unwrap();
    assert_eq!(*result.downcast::<i32>().unwrap(), 5);
    let result = descriptor.invoke_named(&mut counter, "reset", &[]).unwrap();
    assert!(result.is::<()>());
    assert_eq!(counter.value, 0);
}

#[test]
fn unknown_names_are_errors_and_case_sensitive() {
    let descriptor = reflection();
    let mut counter = Counter::default();
    assert_eq!(
        descriptor.field_index("Name").unwrap_err().to_string(),
        "unknown field `Name`"
    );
    assert_eq!(
        descriptor.method_index("Add").unwrap_err().to_string(),
        "unknown method `Add`"
    );
    assert!(descriptor.read_named(&counter, "missing").is_err());
    assert!(
        descriptor
            .invoke_named(&mut counter, "missing", &[])
            .is_err()
    );
}

#[test]
fn invalid_indices_return_errors_without_mutation() {
    let descriptor = reflection();
    let mut counter = Counter::default();
    for index in [2, usize::MAX] {
        assert!(descriptor.read(&counter, FieldIndex(index)).is_err());
    }
    for index in [4, usize::MAX] {
        assert!(
            descriptor
                .invoke(&mut counter, MethodIndex(index), &[])
                .is_err()
        );
    }
    assert_eq!(counter.value, 0);
}

#[test]
fn incorrect_arity_is_rejected_before_dispatch() {
    let descriptor = reflection();
    let mut counter = Counter::default();
    assert_eq!(
        descriptor
            .invoke_named(&mut counter, "add", &[])
            .unwrap_err()
            .to_string(),
        "method `add` expects 1 arguments, got 0"
    );
    assert!(
        descriptor
            .invoke_named(&mut counter, "add", &[&1_i32, &2_i32])
            .is_err()
    );
    assert!(
        descriptor
            .invoke_named(&mut counter, "reset", &[&1_i32])
            .is_err()
    );
    assert_eq!(counter.value, 0);
}

#[test]
fn adapter_type_errors_and_errors_propagate() {
    let descriptor = reflection();
    let mut counter = Counter::default();
    assert_eq!(
        descriptor
            .invoke_named(&mut counter, "add", &[&"wrong"])
            .unwrap_err()
            .to_string(),
        "argument 0 must have type `i32`"
    );
    assert_eq!(counter.value, 0);
    assert_eq!(
        descriptor
            .invoke_named(&mut counter, "fail", &[])
            .unwrap_err()
            .to_string(),
        "adapter failed"
    );
}

#[test]
fn argument_helper_checks_missing_and_exact_types() {
    assert_eq!(
        argument::<i32>(&[], 0).unwrap_err().to_string(),
        "missing argument 0"
    );
    let owned = String::from("hello");
    let args: &[&dyn Any] = &[&owned];
    assert_eq!(argument::<String>(args, 0).unwrap(), "hello");
    assert!(argument::<&str>(args, 0).is_err());
}

#[test]
fn registration_rejects_duplicate_and_empty_names() {
    let field = |name| Field::new::<Counter>(name, |c| &c.value);
    let method = |name| Method::new::<Counter>(name, 0, |_, _| Ok(Box::new(())));
    assert_eq!(
        TypeDescriptor::new::<Counter>(vec![field("value"), field("value")], vec![])
            .err()
            .unwrap()
            .to_string(),
        "duplicate field name `value`"
    );
    assert_eq!(
        TypeDescriptor::new::<Counter>(vec![], vec![method("reset"), method("reset")])
            .err()
            .unwrap()
            .to_string(),
        "duplicate method name `reset`"
    );
    assert!(TypeDescriptor::new::<Counter>(vec![field("")], vec![]).is_err());
    assert!(TypeDescriptor::new::<Counter>(vec![], vec![method("")]).is_err());
    assert!(TypeDescriptor::new::<Counter>(vec![field("same")], vec![method("same")]).is_ok());
}

#[test]
fn empty_descriptor_is_valid_but_has_no_members() {
    let descriptor = TypeDescriptor::new::<Counter>(vec![], vec![]).unwrap();
    assert!(descriptor.fields().is_empty());
    assert!(descriptor.methods().is_empty());
    assert!(descriptor.field_index("value").is_err());
    assert!(descriptor.method_index("add").is_err());
}

#[test]
fn descriptor_rejects_wrong_receiver_types() {
    let descriptor = reflection();
    let mut wrong = 10_i32;
    assert_eq!(descriptor.type_id(), std::any::TypeId::of::<Counter>());
    assert_eq!(descriptor.type_name(), std::any::type_name::<Counter>());
    assert!(descriptor.read(&wrong, FieldIndex(0)).is_err());
    assert!(
        descriptor
            .invoke(&mut wrong, MethodIndex(0), &[&1_i32])
            .is_err()
    );
    assert_eq!(wrong, 10);
}

#[test]
fn mismatched_manual_registration_returns_errors() {
    let descriptor = TypeDescriptor::new::<Counter>(
        vec![Field::new::<i32>("wrong", |v| v)],
        vec![Method::new::<i32>("wrong", 0, |_, _| Ok(Box::new(())))],
    )
    .unwrap();
    let mut counter = Counter::default();
    assert!(descriptor.read(&counter, FieldIndex(0)).is_err());
    assert!(
        descriptor
            .invoke(&mut counter, MethodIndex(0), &[])
            .is_err()
    );
}

#[test]
fn dynamic_object_supports_indexed_and_named_operations() {
    use pixui_reflection::DynamicObject;
    static DESCRIPTOR: std::sync::OnceLock<TypeDescriptor> = std::sync::OnceLock::new();
    let descriptor = DESCRIPTOR.get_or_init(reflection);
    let mut object = DynamicObject::new(Counter::default(), descriptor).unwrap();
    let add = object.method_index("add").unwrap();
    let value = object.field_index("value").unwrap();
    object.invoke(add, &[&3_i32]).unwrap();
    object.invoke_named("add", &[&4_i32]).unwrap();
    assert_eq!(object.read(value).unwrap().downcast_ref::<i32>(), Some(&7));
    assert_eq!(
        object.read_named("value").unwrap().downcast_ref::<i32>(),
        Some(&7)
    );
    assert_eq!(object.downcast_ref::<Counter>().unwrap().value, 7);
    assert!(object.downcast_ref::<i32>().is_none());
    assert!(object.downcast_mut::<i32>().is_none());
    object.downcast_mut::<Counter>().unwrap().value = 11;
    assert_eq!(object.read(value).unwrap().downcast_ref::<i32>(), Some(&11));
    assert!(std::ptr::eq(object.descriptor(), descriptor));
    assert!(object.read(FieldIndex(99)).is_err());
    assert!(object.invoke(MethodIndex(99), &[]).is_err());
    assert!(object.read_named("missing").is_err());
    assert!(object.invoke_named("missing", &[]).is_err());
    assert!(object.invoke(add, &[]).is_err());
    assert!(object.invoke(add, &[&"bad"]).is_err());
    assert_eq!(
        object.invoke_named("fail", &[]).unwrap_err().to_string(),
        "adapter failed"
    );
    assert!(DynamicObject::new(10_i32, descriptor).is_err());
}

#[test]
fn heterogeneous_objects_share_a_single_non_generic_api() {
    use pixui_reflection::{DynamicObject, type_descriptor};
    use std::sync::OnceLock;
    struct Label {
        name: String,
    }
    static LABEL: OnceLock<TypeDescriptor> = OnceLock::new();
    static COUNTER: OnceLock<TypeDescriptor> = OnceLock::new();
    let label_descriptor =
        LABEL.get_or_init(|| type_descriptor!(Label, fields: [name], methods: []).unwrap());
    let objects = vec![
        DynamicObject::new(
            Counter {
                name: "counter".into(),
                value: 0,
            },
            COUNTER.get_or_init(reflection),
        )
        .unwrap(),
        DynamicObject::new(
            Label {
                name: "label".into(),
            },
            label_descriptor,
        )
        .unwrap(),
    ];
    let names: Vec<_> = objects
        .iter()
        .map(|object| {
            object
                .read_named("name")
                .unwrap()
                .downcast_ref::<String>()
                .unwrap()
                .as_str()
        })
        .collect();
    assert_eq!(names, ["counter", "label"]);
}

#[test]
fn macro_allows_empty_fields_and_detects_duplicate_methods() {
    let empty = pixui_reflection::type_descriptor!(Counter, fields: [], methods: []).unwrap();
    assert!(empty.fields().is_empty());
    assert!(empty.methods().is_empty());
    assert!(
        pixui_reflection::type_descriptor!(Counter, fields: [value, value], methods: []).is_err()
    );
    assert!(
        pixui_reflection::type_descriptor!(Counter, fields: [], methods: [
            "same" => (0, |_, _| Ok(Box::new(()))),
            "same" => (0, |_, _| Ok(Box::new(()))),
        ])
        .is_err()
    );
}
