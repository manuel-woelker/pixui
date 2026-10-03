use pixui_reflect::{DynamicObject, Reflect};

#[test]
fn descriptor_is_shared_across_threads_and_outlives_objects() {
    let descriptors: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(model::Counter::type_descriptor))
        .map(|thread| thread.join().unwrap())
        .collect();
    let descriptor = descriptors[0];
    assert!(
        descriptors
            .iter()
            .all(|other| std::ptr::eq(descriptor, *other))
    );
    let object = DynamicObject::from_reflect(model::Counter::new());
    let retained = object.descriptor();
    drop(object);
    assert!(std::ptr::eq(retained, descriptor));
    assert!(retained.field_index("value").is_ok());
}

#[pixui_reflect::reflect]
mod model {
    #[derive(Default)]
    #[cfg_attr(all(), derive(Debug))]
    pub struct Counter {
        pub value: i32,
        name: String,
        #[cfg(any())]
        disabled: i32,
        #[cfg_attr(all(), cfg(any()))]
        conditionally_disabled: i32,
    }

    impl Counter {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn add(&mut self, amount: i32) -> i32 {
            self.value += amount;
            self.value
        }
        pub fn rename(&mut self, name: String) {
            self.name = name;
        }
        pub fn matches(&self, name: &str) -> bool {
            self.name == name
        }
        pub fn sum(&self, first: i32, second: &i32) -> i32 {
            first + second
        }
        pub fn result(&self) -> Result<i32, String> {
            Err("application error".into())
        }
        #[cfg(any())]
        pub fn disabled(&self) -> i32 {
            0
        }
        fn private_method(&self) -> usize {
            self.name.len()
        }
    }

    impl Counter {
        pub fn reset(&mut self) {
            self.value = 0;
        }
    }

    pub struct Marker;
    impl Marker {
        pub fn ping(&self) -> bool {
            true
        }
    }
    #[cfg(any())]
    pub struct Disabled;
}

#[test]
fn discovers_fields_and_methods_without_enumeration() {
    let descriptor = model::Counter::type_descriptor();
    assert_eq!(
        descriptor
            .fields()
            .iter()
            .map(|f| f.name)
            .collect::<Vec<_>>(),
        ["value", "name"]
    );
    assert_eq!(
        descriptor
            .methods()
            .iter()
            .map(|m| m.name)
            .collect::<Vec<_>>(),
        [
            "add",
            "rename",
            "matches",
            "sum",
            "result",
            "private_method",
            "reset"
        ]
    );
    assert!(descriptor.method_index("new").is_err());
    assert!(descriptor.method_index("disabled").is_err());
    assert!(std::ptr::eq(descriptor, model::Counter::type_descriptor()));
}

#[test]
fn automatic_object_reads_fields_and_calls_ordinary_methods() {
    let mut object = DynamicObject::from_reflect(model::Counter::new());
    let add = object.method_index("add").unwrap();
    assert_eq!(
        *object
            .invoke(add, &[&7_i32])
            .unwrap()
            .downcast::<i32>()
            .unwrap(),
        7
    );
    assert_eq!(
        object.read_named("value").unwrap().downcast_ref::<i32>(),
        Some(&7)
    );
    let name = String::from("hello");
    assert!(object.invoke_named("rename", &[&name]).unwrap().is::<()>());
    assert_eq!(name, "hello");
    assert_eq!(
        object.read_named("name").unwrap().downcast_ref::<String>(),
        Some(&name)
    );
    assert!(
        *object
            .invoke_named("matches", &[&name])
            .unwrap()
            .downcast::<bool>()
            .unwrap()
    );
    assert_eq!(
        *object
            .invoke_named("sum", &[&2_i32, &3_i32])
            .unwrap()
            .downcast::<i32>()
            .unwrap(),
        5
    );
    assert_eq!(
        *object
            .invoke_named("private_method", &[])
            .unwrap()
            .downcast::<usize>()
            .unwrap(),
        5
    );
    object.invoke_named("reset", &[]).unwrap();
    assert_eq!(object.downcast_ref::<model::Counter>().unwrap().value, 0);
}

#[test]
fn validates_all_arguments_before_method_mutation() {
    let mut object = DynamicObject::from_reflect(model::Counter::new());
    assert!(object.invoke_named("add", &[&"wrong"]).is_err());
    assert!(object.invoke_named("add", &[]).is_err());
    assert!(object.invoke_named("sum", &[&1_i32, &"wrong"]).is_err());
    assert_eq!(object.downcast_ref::<model::Counter>().unwrap().value, 0);
}

#[test]
fn application_results_are_return_values_and_unit_structs_are_supported() {
    let mut object = DynamicObject::from_reflect(model::Counter::new());
    let result = object
        .invoke_named("result", &[])
        .unwrap()
        .downcast::<Result<i32, String>>()
        .unwrap();
    assert_eq!(*result, Err("application error".into()));
    let mut marker = DynamicObject::from_reflect(model::Marker);
    assert!(marker.descriptor().fields().is_empty());
    assert!(
        *marker
            .invoke_named("ping", &[])
            .unwrap()
            .downcast::<bool>()
            .unwrap()
    );
}
