use crate::entity::store::{EntityStore, TypedEntityKey};
use crate::reflection::Reflect;
use pixui_base::result::PixuiResult;

/// Stores application-wide state.
#[derive(Default)]
pub struct Application {
    /// The application's entity storage.
    store: EntityStore,
}

impl Application {
    /// Creates an empty application.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the entity store owned by this application.
    pub fn entity_store(&self) -> &EntityStore {
        &self.store
    }

    /// Returns mutable access to the entity store owned by this application.
    pub fn entity_store_mut(&mut self) -> &mut EntityStore {
        &mut self.store
    }

    /// Registers storage for `E` if needed and inserts a single entity.
    pub fn add_entity<E: Reflect>(&mut self, entity: E) -> PixuiResult<TypedEntityKey<E>> {
        self.store.register_entity_type::<E>()?;
        Ok(self.store.add_entities([entity])?[0])
    }
}

#[cfg(test)]
mod tests {
    use super::Application;
    use facet::Facet;

    #[derive(Debug, Facet, PartialEq, Eq)]
    struct TestEntity {
        name: String,
    }

    #[test]
    fn add_entity_registers_the_type_and_returns_a_typed_key() {
        let mut application = Application::new();

        let entity_key = application
            .add_entity(TestEntity {
                name: String::from("alpha"),
            })
            .unwrap();

        assert_eq!(
            application.entity_store().get_entity(entity_key).unwrap(),
            &TestEntity {
                name: String::from("alpha")
            }
        );
    }
}
