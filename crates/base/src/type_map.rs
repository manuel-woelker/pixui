use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;

#[derive(Default)]
pub struct TypeMap<V> {
    entries: Vec<V>,
    /// map from type id to index in the entries vector
    mapping: HashMap<TypeId, usize>,
}

pub struct TypeKey<V> {
    key: usize,
    _phantom: PhantomData<V>,
}

impl<V> TypeMap<V> {}
