use crate::entity::store::EntityStore;

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
}
