#![doc = include_str!("../README.md")]

mod dynamic_object;
mod field;
mod macros;
mod method;
mod reflect;
mod type_descriptor;

pub use dynamic_object::DynamicObject;
pub use field::{Field, FieldGetter, FieldIndex};
pub use method::{
    Method, MethodIndex, MethodInvoker, MutMethodInvoker, RefMethodInvoker, SharedMethodInvoker,
    argument,
};
pub use pixui_reflect_macros::reflect;
pub use reflect::Reflect;
pub use type_descriptor::TypeDescriptor;
