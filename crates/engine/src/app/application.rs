use crate::app::component_draw_registry::ComponentDrawRegistry;
use crate::app::component_draw_renderer::ComponentDrawRenderer;
use crate::draw::draw_list::DrawList;
use crate::entity::store::{EntityStore, TypedEntityKey};
use crate::reflection::Reflect;
use crate::viewport::Viewport;
use pixui_base::result::PixuiResult;
use pixui_base::shared_string::SharedString;

/// Stores application-wide state.
#[derive(Default)]
pub struct Application {
    /// The application's entity storage.
    store: EntityStore,
    /// Named component renderers that produce draw lists for a viewport.
    component_draw_registry: ComponentDrawRegistry,
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

    /// Registers a named renderer that can produce draw commands for a component.
    pub fn register_component_renderer<R>(
        &mut self,
        component_name: impl Into<SharedString>,
        renderer: R,
    ) where
        R: ComponentDrawRenderer + 'static,
    {
        self.component_draw_registry
            .register(component_name, renderer);
    }

    /// Renders a named component for the provided viewport.
    pub fn render_component(
        &self,
        component_name: &str,
        viewport: &Viewport,
    ) -> PixuiResult<DrawList> {
        self.component_draw_registry
            .render_component(self, component_name, viewport)
    }
}

#[cfg(test)]
mod tests {
    use super::Application;
    use crate::draw::draw_bounds::DrawBounds;
    use crate::draw::draw_list::DrawList;
    use crate::viewport::Viewport;
    use facet::Facet;
    use pixui_base::result::PixuiResult;

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

    #[test]
    fn register_component_renderer_renders_named_components() {
        let mut application = Application::new();
        application.register_component_renderer("CounterApp", render_counter);

        let draw_list = application
            .render_component("CounterApp", &Viewport::new(320.0, 240.0, 1.0))
            .unwrap();

        assert_eq!(draw_list.bounds, DrawBounds::new(0.0, 0.0, 320.0, 240.0));
    }

    #[test]
    fn render_component_returns_context_for_unknown_component_names() {
        let application = Application::new();

        let error = application
            .render_component("Missing", &Viewport::new(320.0, 240.0, 1.0))
            .unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("no such component renderer: Missing")
        );
    }

    fn render_counter(_application: &Application, viewport: &Viewport) -> PixuiResult<DrawList> {
        Ok(DrawList::new(DrawBounds::new(
            0.0,
            0.0,
            viewport.width,
            viewport.height,
        )))
    }
}
