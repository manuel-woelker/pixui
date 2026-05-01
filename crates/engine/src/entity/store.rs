use crate::reflection::Reflect;
use pixui_base::result::{OptionExt, PixuiResult};
use pixui_base::type_map::{TypeKey, TypeMap};
use slotmap::{Key, SlotMap, new_key_type};
use std::any::{Any, type_name};
use std::marker::PhantomData;

/// Stores registered entity slices keyed by entity type.
#[derive(Default)]
pub struct EntityStore {
    /// Type-indexed entity slices, one per registered entity type.
    slices: TypeMap<Box<dyn DynEntitySlice>>,
}

new_key_type! {
    struct EntityKey;
}

#[derive(Debug, Copy, Clone)]
pub struct SliceId(u32);

/// Identifies a single entity in the store.
#[derive(Debug, Copy, Clone)]
pub struct EntityId {
    /// The slice that owns the entity.
    slice_id: SliceId,
    /// The slotmap key inside that slice.
    key: EntityKey,
}

impl EntityId {
    /// Returns the slice that owns this entity.
    #[must_use]
    pub fn slice_id(&self) -> SliceId {
        self.slice_id
    }

    /// Returns the raw slot key for debugging and persistence-adjacent code.
    #[must_use]
    pub fn key_data(&self) -> u64 {
        self.key.data().as_ffi()
    }
}

/// A typed handle to an entity stored in an [`EntityStore`].
#[derive(Debug)]
pub struct TypedEntityKey<E: Reflect> {
    /// The untyped entity identifier used by the backing store.
    entity_id: EntityId,
    /// Tracks the entity type without storing a runtime value.
    entity_marker: PhantomData<fn() -> E>,
}

impl<E: Reflect> Copy for TypedEntityKey<E> {}

impl<E: Reflect> Clone for TypedEntityKey<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E: Reflect> TypedEntityKey<E> {
    /// Returns the slice that owns this entity.
    #[must_use]
    pub fn slice_id(&self) -> SliceId {
        self.entity_id.slice_id()
    }

    /// Returns the raw slot key for debugging and persistence-adjacent code.
    #[must_use]
    pub fn key_data(&self) -> u64 {
        self.entity_id.key_data()
    }
}

impl<E: Reflect> From<TypedEntityKey<E>> for EntityId {
    fn from(value: TypedEntityKey<E>) -> Self {
        value.entity_id
    }
}

trait DynEntitySlice: Any {}

impl<E: Reflect> DynEntitySlice for EntitySlice<E> {}

struct EntitySlice<E: Reflect> {
    /// The public slice id associated with this typed slice.
    slice_id: SliceId,
    /// Entities stored for a single reflected type.
    entities: SlotMap<EntityKey, E>,
}

impl<E: Reflect> EntitySlice<E> {
    pub fn new(slice_id: SliceId) -> Self {
        Self {
            slice_id,
            entities: SlotMap::with_key(),
        }
    }

    fn add_entity(&mut self, entity: E) -> TypedEntityKey<E> {
        let key = self.entities.insert(entity);
        TypedEntityKey {
            entity_id: EntityId {
                slice_id: self.slice_id,
                key,
            },
            entity_marker: PhantomData,
        }
    }
}

type SliceKey = TypeKey<Box<dyn DynEntitySlice>>;

/* 📖 # Why does `SliceId` stay separate from `TypeKey`?
`TypeMap` is the internal storage primitive, but `EntityStore` exposes a
domain-specific `SliceId` so callers do not need to know how slices are
indexed. That keeps the public API stable even if the internal container
changes later.
*/

impl SliceId {
    fn from_key(key: SliceKey) -> Self {
        Self(key.index() as u32)
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl EntityStore {
    /// Registers storage for entity type `E`.
    ///
    /// Registering the same type more than once returns the existing slice id.
    pub fn register_entity_type<E: Reflect>(&mut self) -> PixuiResult<SliceId> {
        if let Some(key) = self.slices.key::<E>() {
            return Ok(SliceId::from_key(key));
        }

        let slice_id = SliceId(self.slices.len() as u32);
        let key = self
            .slices
            .insert::<E>(Box::new(EntitySlice::<E>::new(slice_id)));
        Ok(SliceId::from_key(key))
    }

    /// Appends entities of type `E` to the previously registered slice.
    pub fn add_entities<E: Reflect>(
        &mut self,
        entities: impl IntoIterator<Item = E>,
    ) -> PixuiResult<Vec<TypedEntityKey<E>>> {
        let slice = self.get_slice_mut::<E>()?;
        Ok(entities
            .into_iter()
            .map(|entity| slice.add_entity(entity))
            .collect())
    }

    /// Loads an entity using a typed entity key.
    pub fn get_entity<E: Reflect>(&self, entity_id: TypedEntityKey<E>) -> PixuiResult<&E> {
        self.get_entity_untyped::<E>(entity_id)
    }

    /// Loads a mutable entity using a typed entity key.
    pub fn get_entity_mut<E: Reflect>(
        &mut self,
        entity_id: TypedEntityKey<E>,
    ) -> PixuiResult<&mut E> {
        self.get_entity_untyped_mut::<E>(entity_id)
    }

    /// Loads an entity using an untyped id while still requiring the expected entity type.
    pub fn get_entity_untyped<E: Reflect>(
        &self,
        entity_id: impl Into<EntityId>,
    ) -> PixuiResult<&E> {
        let entity_id = entity_id.into();
        let slice_id = entity_id.slice_id.index();
        let dyn_slice = self
            .slices
            .get_by_key(TypeKey::from(slice_id))
            .with_context(|| format!("Entity store does not contain {}", slice_id))?;
        let slice = (dyn_slice.as_ref() as &dyn Any)
            .downcast_ref::<EntitySlice<E>>()
            .with_context(|| format!("Failed to downcast {}", type_name::<E>()))?;
        let entity = slice
            .entities
            .get(entity_id.key)
            .with_context(|| format!("No entry for key {}", entity_id.key_data()))?;
        Ok(entity)
    }

    /// Loads a mutable entity using an untyped id while still requiring the expected entity type.
    pub fn get_entity_untyped_mut<E: Reflect>(
        &mut self,
        entity_id: impl Into<EntityId>,
    ) -> PixuiResult<&mut E> {
        let entity_id = entity_id.into();
        let slice = self.get_slice_mut::<E>()?;
        let entity = slice
            .entities
            .get_mut(entity_id.key)
            .with_context(|| format!("No entry for key {}", entity_id.key_data()))?;
        Ok(entity)
    }

    fn get_slice_mut<E: Reflect>(&mut self) -> PixuiResult<&mut EntitySlice<E>> {
        let slice = self
            .slices
            .get_mut::<E>()
            .with_context(|| format!("Entity store does not contain {}", type_name::<E>()))?;
        (slice.as_mut() as &mut dyn Any)
            .downcast_mut::<EntitySlice<E>>()
            .with_context(|| format!("Failed to downcast {}", type_name::<E>()))
    }
}

#[cfg(test)]
mod tests {
    use super::{EntityId, EntityStore};
    use facet::Facet;

    #[derive(Debug, Facet, PartialEq, Eq)]
    struct TestEntity {
        name: String,
        health: u32,
    }

    #[test]
    fn register_entity_type_returns_the_same_slice_id_for_the_same_type() {
        let mut store = EntityStore::default();

        let first_id = store.register_entity_type::<TestEntity>().unwrap();
        let second_id = store.register_entity_type::<TestEntity>().unwrap();

        assert_eq!(first_id.0, second_id.0);
    }

    #[test]
    fn get_entity_loads_values_with_typed_entity_keys() {
        let mut store = EntityStore::default();
        let slice_id = store.register_entity_type::<TestEntity>().unwrap();

        let entity_ids = store
            .add_entities([
                TestEntity {
                    name: "alpha".into(),
                    health: 10,
                },
                TestEntity {
                    name: "beta".into(),
                    health: 20,
                },
            ])
            .unwrap();

        assert_eq!(entity_ids.len(), 2);
        assert_eq!(entity_ids[0].slice_id().0, slice_id.0);
        assert_eq!(entity_ids[1].slice_id().0, slice_id.0);
        assert_ne!(entity_ids[0].key_data(), entity_ids[1].key_data());
        assert_eq!(
            store.get_entity::<TestEntity>(entity_ids[0]).unwrap(),
            &TestEntity {
                name: "alpha".into(),
                health: 10,
            }
        );
        assert_eq!(
            store.get_entity::<TestEntity>(entity_ids[1]).unwrap(),
            &TestEntity {
                name: "beta".into(),
                health: 20,
            }
        );
    }

    #[test]
    fn get_entity_untyped_loads_values_with_untyped_entity_ids() {
        let mut store = EntityStore::default();
        store.register_entity_type::<TestEntity>().unwrap();

        let entity_id = store
            .add_entities([TestEntity {
                name: "alpha".into(),
                health: 10,
            }])
            .unwrap()[0];

        let untyped_entity_id = EntityId::from(entity_id);

        assert_eq!(untyped_entity_id.slice_id().0, entity_id.slice_id().0);
        assert_eq!(untyped_entity_id.key_data(), entity_id.key_data());
        assert_eq!(
            store
                .get_entity_untyped::<TestEntity>(untyped_entity_id)
                .unwrap(),
            &TestEntity {
                name: "alpha".into(),
                health: 10,
            }
        );
    }

    #[test]
    fn add_entities_requires_the_type_to_be_registered_first() {
        let mut store = EntityStore::default();

        let error = store
            .add_entities([TestEntity {
                name: "unregistered".into(),
                health: 1,
            }])
            .unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("Entity store does not contain")
        );
        assert!(error.to_test_string().contains("TestEntity"));
    }

    #[test]
    fn get_entity_mut_updates_values_with_typed_entity_keys() {
        let mut store = EntityStore::default();
        store.register_entity_type::<TestEntity>().unwrap();

        let entity_key = store
            .add_entities([TestEntity {
                name: "alpha".into(),
                health: 10,
            }])
            .unwrap()[0];

        store.get_entity_mut(entity_key).unwrap().health = 25;

        assert_eq!(store.get_entity(entity_key).unwrap().health, 25);
    }
}
