//! Descriptors for common scalar values used as constructor inputs.

use crate::{Reflect, TypeDescriptor};
use std::sync::OnceLock;

macro_rules! scalars {
    ($($ty:ty),*) => { $(
        impl Reflect for $ty {
            fn type_descriptor() -> &'static TypeDescriptor {
                static DESCRIPTOR: OnceLock<TypeDescriptor> = OnceLock::new();
                DESCRIPTOR.get_or_init(|| TypeDescriptor::new::<Self>(vec![], vec![])
                    .expect("scalar descriptors have no members"))
            }
        }
    )* };
}

scalars!(
    String,
    bool,
    char,
    u128,
    i128,
    u8,
    u16,
    u32,
    u64,
    usize,
    i8,
    i16,
    i32,
    i64,
    isize,
    f32,
    f64,
    ()
);
