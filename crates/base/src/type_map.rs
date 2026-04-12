//! A small map that stores one value per Rust type.

use std::any::TypeId;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::marker::PhantomData;

/// Stores at most one value for each concrete `'static` type.
///
/// `TypeMap` is useful when values are indexed by a compile-time type but all
/// stored values share a common runtime representation.
///
/// Values are stored densely in a vector. Once a type is inserted, its
/// [`TypeKey`] stays stable even if later inserts add more entries or replace
/// the value for that same type.
pub struct TypeMap<V> {
    entries: Vec<V>,
    /// map from type id to index in the entries vector
    mapping: HashMap<TypeId, usize>,
}

/// A stable key for a value stored inside a [`TypeMap`].
///
/// The key is parameterized by the mapped value type so keys from unrelated
/// maps cannot be mixed accidentally.
pub struct TypeKey<V> {
    key: usize,
    _phantom: PhantomData<V>,
}

impl<V> Copy for TypeKey<V> {}

impl<V> Clone for TypeKey<V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V> std::fmt::Debug for TypeKey<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeKey").field("key", &self.key).finish()
    }
}

impl<V> PartialEq for TypeKey<V> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<V> Eq for TypeKey<V> {}

impl<V> std::hash::Hash for TypeKey<V> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl<V> TypeMap<V> {
    /// Creates an empty type map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of distinct types stored in the map.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the map contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns `true` if a value has been stored for `T`.
    #[must_use]
    pub fn contains<T: 'static>(&self) -> bool {
        self.mapping.contains_key(&TypeId::of::<T>())
    }

    /// Returns the stable key for `T` if the type is present in the map.
    #[must_use]
    pub fn key<T: 'static>(&self) -> Option<TypeKey<V>> {
        self.mapping
            .get(&TypeId::of::<T>())
            .copied()
            .map(TypeKey::from_index)
    }

    /// Inserts a value for `T`.
    ///
    /// If `T` is already present, the existing value is replaced and its stable
    /// key is returned unchanged.
    pub fn insert<T: 'static>(&mut self, value: V) -> TypeKey<V> {
        match self.mapping.entry(TypeId::of::<T>()) {
            Entry::Occupied(entry) => {
                let index = *entry.get();
                self.entries[index] = value;
                TypeKey::from_index(index)
            }
            Entry::Vacant(entry) => {
                let index = self.entries.len();
                self.entries.push(value);
                entry.insert(index);
                TypeKey::from_index(index)
            }
        }
    }

    /// Returns the value stored for `T`.
    #[must_use]
    pub fn get<T: 'static>(&self) -> Option<&V> {
        self.key::<T>().and_then(|key| self.get_by_key(key))
    }

    /// Returns the mutable value stored for `T`.
    #[must_use]
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut V> {
        let index = self.mapping.get(&TypeId::of::<T>()).copied()?;
        self.entries.get_mut(index)
    }

    /// Returns the value stored at `key`.
    #[must_use]
    pub fn get_by_key(&self, key: TypeKey<V>) -> Option<&V> {
        self.entries.get(key.key)
    }

    /// Returns the mutable value stored at `key`.
    #[must_use]
    pub fn get_by_key_mut(&mut self, key: TypeKey<V>) -> Option<&mut V> {
        self.entries.get_mut(key.key)
    }

    fn insert_with_key<T: 'static>(&mut self, value: V) -> (TypeKey<V>, &mut V) {
        match self.mapping.entry(TypeId::of::<T>()) {
            Entry::Occupied(entry) => {
                let index = *entry.get();
                self.entries[index] = value;
                let key = TypeKey::from_index(index);
                (key, &mut self.entries[index])
            }
            Entry::Vacant(entry) => {
                let index = self.entries.len();
                self.entries.push(value);
                entry.insert(index);
                let key = TypeKey::from_index(index);
                (key, self.entries.last_mut().unwrap())
            }
        }
    }
}

impl<V: Default> TypeMap<V> {
    /// Returns the value stored for `T`, inserting `V::default()` if needed.
    pub fn get_or_insert_default<T: 'static>(&mut self) -> &V {
        let (_, value) = self.get_or_insert_default_with_key_mut::<T>();
        value
    }

    /// Returns the mutable value stored for `T`, inserting `V::default()` if needed.
    pub fn get_or_insert_default_mut<T: 'static>(&mut self) -> &mut V {
        let (_, value) = self.get_or_insert_default_with_key_mut::<T>();
        value
    }

    /// Returns the stable key and value for `T`, inserting `V::default()` if needed.
    pub fn get_or_insert_default_with_key<T: 'static>(&mut self) -> (TypeKey<V>, &V) {
        let (key, value) = self.get_or_insert_default_with_key_mut::<T>();
        (key, value)
    }

    /// Returns the stable key and mutable value for `T`, inserting `V::default()` if needed.
    pub fn get_or_insert_default_with_key_mut<T: 'static>(&mut self) -> (TypeKey<V>, &mut V) {
        if let Some(index) = self.mapping.get(&TypeId::of::<T>()).copied() {
            let key = TypeKey::from_index(index);
            return (key, &mut self.entries[index]);
        }

        self.insert_with_key::<T>(V::default())
    }
}

impl<V> Default for TypeMap<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            mapping: HashMap::new(),
        }
    }
}

impl<V> TypeKey<V> {
    fn from_index(key: usize) -> Self {
        Self {
            key,
            _phantom: PhantomData,
        }
    }

    /// Returns the key's raw index inside the backing storage.
    ///
    /// This is mainly useful for debugging and tests.
    #[must_use]
    pub fn index(&self) -> usize {
        self.key
    }
}

impl<V> std::fmt::Display for TypeKey<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.key)
    }
}

impl<V> From<TypeKey<V>> for usize {
    fn from(value: TypeKey<V>) -> Self {
        value.key
    }
}

impl<V> From<usize> for TypeKey<V> {
    fn from(value: usize) -> Self {
        Self::from_index(value)
    }
}

/* 📖 # Why does `TypeMap` store values in a `Vec` instead of directly in the hash map?
The map only needs `TypeId -> index` lookup. Keeping values in a dense vector
makes `TypeKey` cheap and stable, avoids storing `TypeId` next to each value,
and gives direct indexed access once the type has been resolved.
*/

#[cfg(test)]
mod tests {
    use super::TypeMap;

    struct Alpha;
    struct Beta;

    #[test]
    fn insert_and_get_use_the_requested_type() {
        let mut map = TypeMap::new();

        map.insert::<Alpha>("alpha");
        map.insert::<Beta>("beta");

        assert_eq!(map.get::<Alpha>(), Some(&"alpha"));
        assert_eq!(map.get::<Beta>(), Some(&"beta"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn insert_replaces_existing_values_without_changing_the_key() {
        let mut map = TypeMap::new();

        let first_key = map.insert::<Alpha>("before");
        let second_key = map.insert::<Alpha>("after");

        assert_eq!(first_key, second_key, "keys should stay stable");
        assert_eq!(map.get::<Alpha>(), Some(&"after"));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn get_mut_updates_values_in_place() {
        let mut map = TypeMap::new();
        map.insert::<Alpha>(String::from("alpha"));

        map.get_mut::<Alpha>().unwrap().push_str("-updated");

        assert_eq!(
            map.get::<Alpha>().map(String::as_str),
            Some("alpha-updated")
        );
    }

    #[test]
    fn get_by_key_reads_the_same_entry_as_type_lookup() {
        let mut map = TypeMap::new();
        let alpha_key = map.insert::<Alpha>(41_u32);

        *map.get_by_key_mut(alpha_key).unwrap() += 1;

        assert_eq!(map.get_by_key(alpha_key), Some(&42));
        assert_eq!(map.get::<Alpha>(), Some(&42));
    }

    #[test]
    fn missing_types_return_none() {
        let mut map = TypeMap::<u32>::new();
        map.insert::<Alpha>(1);

        assert!(!map.is_empty());
        assert!(map.contains::<Alpha>());
        assert!(!map.contains::<Beta>());
        assert_eq!(map.key::<Beta>(), None);
        assert_eq!(map.get::<Beta>(), None);
    }

    #[test]
    fn type_key_debug_and_display_use_the_underlying_index() {
        let mut map = TypeMap::new();
        let alpha_key = map.insert::<Alpha>(10_u32);

        assert_eq!(alpha_key.index(), 0);
        assert_eq!(alpha_key.to_string(), "0");
        assert!(format!("{alpha_key:?}").contains("0"));
    }

    #[test]
    fn get_or_insert_default_inserts_when_missing() {
        let mut map = TypeMap::<String>::new();

        let value = map.get_or_insert_default::<Alpha>();

        assert_eq!(value, "");
        assert_eq!(map.get::<Alpha>().map(String::as_str), Some(""));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn get_or_insert_default_does_not_replace_existing_values() {
        let mut map = TypeMap::<String>::new();
        map.insert::<Alpha>("kept".into());

        let value = map.get_or_insert_default::<Alpha>();

        assert_eq!(value, "kept");
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn get_or_insert_default_mut_returns_mutable_access() {
        let mut map = TypeMap::<Vec<u32>>::new();

        map.get_or_insert_default_mut::<Alpha>().push(42);

        assert_eq!(map.get::<Alpha>(), Some(&vec![42]));
    }

    #[test]
    fn get_or_insert_default_with_key_variants_return_the_stable_key() {
        let mut map = TypeMap::<String>::new();

        let (first_key, first_value) = map.get_or_insert_default_with_key::<Alpha>();
        assert_eq!(first_key.index(), 0);
        assert_eq!(first_value, "");

        let (second_key, second_value) = map.get_or_insert_default_with_key_mut::<Alpha>();
        second_value.push_str("value");

        assert_eq!(first_key, second_key);
        assert_eq!(
            map.get_by_key(second_key).map(String::as_str),
            Some("value")
        );
    }
}
