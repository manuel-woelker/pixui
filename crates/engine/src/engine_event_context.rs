use crate::app::Application;
use crate::entity::store::TypedEntityKey;
use crate::reflection::Reflect;
use pixui_base::result::PixuiResult;

/// Mutable context passed to an engine event handler.
pub struct EngineEventContext<'a, E> {
    /// Application state owned by the engine.
    application: &'a mut Application,
    /// Event being handled.
    pub event: &'a mut E,
}

impl<'a, E> EngineEventContext<'a, E> {
    /// Creates a context for a single handler invocation.
    pub(crate) fn new(application: &'a mut Application, event: &'a mut E) -> Self {
        Self { application, event }
    }

    /// Returns shared access to the application state.
    pub fn application(&self) -> &Application {
        self.application
    }

    /// Returns mutable access to the application state.
    pub fn application_mut(&mut self) -> &mut Application {
        self.application
    }

    /// Loads an entity using a typed entity key.
    pub fn get_entity<T: Reflect>(&self, entity_key: TypedEntityKey<T>) -> PixuiResult<&T> {
        self.application.entity_store().get_entity(entity_key)
    }

    /// Loads a mutable entity using a typed entity key.
    pub fn get_entity_mut<T: Reflect>(
        &mut self,
        entity_key: TypedEntityKey<T>,
    ) -> PixuiResult<&mut T> {
        self.application
            .entity_store_mut()
            .get_entity_mut(entity_key)
    }
}
