use crate::reflection::Reflect;
use pixui_base::result::{OptionExt, PixuiResult};
use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::collections::hash_map::Entry;

#[derive(Debug, Copy, Clone)]
pub struct SliceId(u32);

#[derive(Debug, Copy, Clone)]
pub struct EntityId {
    slice_id: SliceId,
    index: u32,
}

impl EntityId {
    #[must_use]
    pub fn slice_id(&self) -> SliceId {
        self.slice_id
    }

    #[must_use]
    pub fn index(&self) -> u32 {
        self.index
    }
}

#[derive(Default)]
pub struct EntityStore {
    slice_map: HashMap<TypeId, SliceId>,
    slices: Vec<Box<dyn DynEntitySlice>>,
}

trait DynEntitySlice: Any {}

impl<E: Reflect> DynEntitySlice for EntitySlice<E> {}

struct EntitySlice<E: Reflect> {
    slice_id: SliceId,
    entities: Vec<Option<E>>,
}

impl<E: Reflect> EntitySlice<E> {
    pub fn new(slice_id: SliceId) -> Self {
        Self {
            slice_id,
            entities: Vec::new(),
        }
    }

    fn add_entity(&mut self, entity: E) -> EntityId {
        let index = self.entities.len();
        self.entities.push(Some(entity));
        EntityId {
            slice_id: self.slice_id,
            index: index as u32,
        }
    }
}

impl EntityStore {
    pub fn register_entity_type<E: Reflect>(&mut self) -> PixuiResult<SliceId> {
        let entity_type_id = match self.slice_map.entry(TypeId::of::<E>()) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let slice_id = SliceId(self.slices.len() as u32);
                entry.insert(slice_id);
                self.slices.push(Box::new(EntitySlice::<E>::new(slice_id)));
                slice_id
            }
        };
        Ok(entity_type_id)
    }

    fn get_slice_id<E: Reflect>(&self) -> PixuiResult<SliceId> {
        Ok(*self
            .slice_map
            .get(&TypeId::of::<E>())
            .with_context(|| format!("Entity store does not contain {}", type_name::<E>()))?)
    }

    pub fn add_entities<E: Reflect>(
        &mut self,
        entities: impl IntoIterator<Item = E>,
    ) -> PixuiResult<Vec<EntityId>> {
        let slice = self.get_slice_mut::<E>()?;
        Ok(entities
            .into_iter()
            .map(|entity| slice.add_entity(entity))
            .collect())
    }

    pub fn get_entity<E: Reflect>(&self, entity_id: EntityId) -> PixuiResult<&E> {
        let slice_id = entity_id.slice_id.0 as usize;
        let dyn_slice = self
            .slices
            .get(slice_id)
            .with_context(|| format!("Entity store does not contain {}", slice_id))?;
        let slice = (dyn_slice.as_ref() as &dyn Any)
            .downcast_ref::<EntitySlice<E>>()
            .with_context(|| format!("Failed to downcast {}", type_name::<E>()))?;
        let index = entity_id.index as usize;
        let entry = slice.entities.get(index);
        let entry2 = entry.with_context(|| format!("No entry at index {index}"))?;
        let entity = entry2
            .as_ref()
            .with_context(|| format!("Entry at index {index} was remove"))?;
        Ok(entity)
    }

    fn get_slice_mut<E: Reflect>(&mut self) -> PixuiResult<&mut EntitySlice<E>> {
        let slice_id = self.get_slice_id::<E>()?;
        let slice = self
            .slices
            .get_mut(slice_id.0 as usize)
            .with_context(|| format!("Entity store does not contain {}", type_name::<E>()))?;
        (slice.as_mut() as &mut dyn Any)
            .downcast_mut::<EntitySlice<E>>()
            .with_context(|| format!("Failed to downcast {}", type_name::<E>()))
    }
}

#[cfg(test)]
mod tests {
    use super::EntityStore;
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
    fn add_entities_stores_values_that_can_be_loaded_again() {
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
        assert_eq!(entity_ids[0].index(), 0);
        assert_eq!(entity_ids[1].slice_id().0, slice_id.0);
        assert_eq!(entity_ids[1].index(), 1);
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
}
