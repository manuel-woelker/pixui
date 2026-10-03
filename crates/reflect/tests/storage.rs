use pixui_reflect::{DynamicObject, Reflect};

#[pixui_reflect::reflect]
mod model {
    pub struct Child {
        pub value: i32,
    }
    impl Child {
        pub fn value(&self) -> i32 {
            self.value
        }
        pub fn add(&mut self, amount: i32) {
            self.value += amount;
        }
    }
    pub struct Parent {
        pub child: Child,
    }
    impl Parent {
        pub fn child(&self) -> &Child {
            &self.child
        }
        pub fn child_mut(&mut self) -> &mut Child {
            &mut self.child
        }
        pub fn checked_child(&self, expected: i32) -> &Child {
            assert_eq!(expected, self.child.value);
            &self.child
        }
    }
}

fn parent() -> model::Parent {
    model::Parent {
        child: model::Child { value: 3 },
    }
}

#[test]
fn owned_objects_are_static_but_borrowed_results_follow_receiver() {
    let mut object: DynamicObject<'static> = DynamicObject::from_reflect(parent());
    assert!(object.is_owned());
    assert!(object.is_mutable());
    let child_index = object.method_index("child").unwrap();
    {
        let mut child = object.invoke_ref(child_index, &[]).unwrap();
        assert!(!child.is_owned());
        assert!(!child.is_mutable());
        assert_eq!(
            child.read_named("value").unwrap().downcast_ref::<i32>(),
            Some(&3)
        );
        assert_eq!(
            *child
                .invoke_shared_named("value", &[])
                .unwrap()
                .downcast::<i32>()
                .unwrap(),
            3
        );
        // `&mut DynamicObject` cannot grant mutable access to a shared value.
        assert!(child.invoke_named("add", &[&1_i32]).is_err());
        assert!(child.downcast_mut::<model::Child>().is_none());
    }
    let child_index = object.method_index("child_mut").unwrap();
    {
        let mut child = object.invoke_mut(child_index, &[]).unwrap();
        assert!(!child.is_owned());
        assert!(child.is_mutable());
        child.invoke_named("add", &[&4_i32]).unwrap();
        child.downcast_mut::<model::Child>().unwrap().value += 1;
    }
    assert_eq!(
        object.downcast_ref::<model::Parent>().unwrap().child.value,
        8
    );
}

#[test]
fn shared_and_mutable_constructors_borrow_original_values() {
    let mut value = model::Child { value: 3 };
    {
        let shared = DynamicObject::from_ref(&value);
        assert!(std::ptr::eq(
            shared.downcast_ref::<model::Child>().unwrap(),
            &value
        ));
        assert_eq!(
            *shared
                .invoke_shared_named("value", &[])
                .unwrap()
                .downcast::<i32>()
                .unwrap(),
            3
        );
    }
    {
        let mut mutable = DynamicObject::from_mut(&mut value);
        mutable.invoke_named("add", &[&2_i32]).unwrap();
        assert_eq!(
            mutable.read_named("value").unwrap().downcast_ref::<i32>(),
            Some(&5)
        );
    }
    assert_eq!(value.value, 5);
    assert!(DynamicObject::borrow(&value, model::Parent::type_descriptor()).is_err());
    assert!(DynamicObject::borrow_mut(&mut value, model::Parent::type_descriptor()).is_err());
}

#[test]
fn mixed_storage_uses_one_public_type() {
    let shared = model::Child { value: 2 };
    let mut mutable = model::Child { value: 3 };
    let mut objects = vec![
        DynamicObject::from_reflect(model::Child { value: 1 }),
        DynamicObject::from_ref(&shared),
        DynamicObject::from_mut(&mut mutable),
    ];
    let values: Vec<_> = objects
        .iter()
        .map(|object| {
            *object
                .invoke_shared_named("value", &[])
                .unwrap()
                .downcast::<i32>()
                .unwrap()
        })
        .collect();
    assert_eq!(values, [1, 2, 3]);
    for object in &mut objects {
        assert_eq!(
            object.invoke_named("add", &[&10_i32]).is_ok(),
            object.is_mutable()
        );
    }
    drop(objects);
    assert_eq!(shared.value, 2);
    assert_eq!(mutable.value, 13);
}

#[test]
fn wrong_invocation_kinds_and_arguments_return_errors() {
    let mut object = DynamicObject::from_reflect(parent());
    assert!(object.invoke_named("child", &[]).is_err());
    assert!(object.invoke_mut_named("child", &[]).is_err());
    assert!(object.invoke_ref_named("child_mut", &[]).is_err());
    assert!(object.invoke_ref_named("checked_child", &[]).is_err());
    assert!(
        object
            .invoke_ref_named("checked_child", &[&"wrong"])
            .is_err()
    );
    assert!(object.invoke_ref_named("missing", &[]).is_err());
    assert!(object.invoke_mut_named("missing", &[]).is_err());
    assert!(
        object
            .invoke_ref(pixui_reflect::MethodIndex(99), &[])
            .is_err()
    );
    let argument = 3_i32;
    assert!(
        object
            .invoke_ref_named("checked_child", &[&argument])
            .is_ok()
    );
    let mut shared = DynamicObject::from_ref(object.downcast_ref::<model::Parent>().unwrap());
    assert!(shared.invoke_mut_named("child_mut", &[]).is_err());
}

#[test]
fn only_owned_storage_drops_the_underlying_value() {
    use pixui_reflect::TypeDescriptor;
    use std::{cell::Cell, rc::Rc, sync::OnceLock};
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    static DESCRIPTOR: OnceLock<TypeDescriptor> = OnceLock::new();
    let descriptor =
        DESCRIPTOR.get_or_init(|| TypeDescriptor::new::<Probe>(vec![], vec![]).unwrap());
    let drops = Rc::new(Cell::new(0));
    let mut probe = Probe(drops.clone());
    drop(DynamicObject::borrow(&probe, descriptor).unwrap());
    drop(DynamicObject::borrow_mut(&mut probe, descriptor).unwrap());
    assert_eq!(drops.get(), 0);
    drop(probe);
    assert_eq!(drops.get(), 1);
    drop(DynamicObject::new(Probe(drops.clone()), descriptor).unwrap());
    assert_eq!(drops.get(), 2);
}

#[test]
fn borrowed_results_outlive_arguments_and_descriptor_dispatch_matches_objects() {
    let mut parent = parent();
    let descriptor = model::Parent::type_descriptor();
    let child = {
        let expected = 3_i32;
        descriptor
            .invoke_ref_named(&parent, "checked_child", &[&expected])
            .unwrap()
    };
    assert_eq!(child.downcast_ref::<model::Child>().unwrap().value, 3);
    drop(child);
    {
        let mut child = descriptor
            .invoke_mut_named(&mut parent, "child_mut", &[])
            .unwrap();
        child.invoke_named("add", &[&1_i32]).unwrap();
    }
    let descriptor = model::Child::type_descriptor();
    assert_eq!(
        *descriptor
            .invoke_shared_named(&parent.child, "value", &[])
            .unwrap()
            .downcast::<i32>()
            .unwrap(),
        4
    );
    assert!(
        descriptor
            .invoke_shared_named(&parent.child, "add", &[&1_i32])
            .is_err()
    );
    assert!(descriptor.invoke_ref_named(&parent, "value", &[]).is_err());
}
