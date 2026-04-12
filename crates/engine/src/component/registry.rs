use crate::component::Component;
use pixui_base::result::{OptionExt, PixuiResult};
use pixui_base::type_map::TypeMap;

#[derive(Debug, Copy, Clone)]
pub struct ComponentId(usize);

impl ComponentId {
    fn from_index(index: usize) -> Self {
        Self(index)
    }
}

pub struct ComponentHolder {}

#[derive(Default)]
pub struct ComponentRegistry {
    components: TypeMap<ComponentHolder>,
}

impl ComponentRegistry {
    pub fn register<C: Component + 'static>(&mut self) -> ComponentId {
        let key = self.components.insert::<C>(ComponentHolder {});
        ComponentId::from_index(key.index())
    }

    pub fn get(&self, id: ComponentId) -> PixuiResult<&ComponentHolder> {
        self.components
            .get_by_key(id.0.into())
            .with_context(|| format!("no such component: {:?}", id.0))
    }
}

#[cfg(test)]
mod tests {
    use super::{ComponentId, ComponentRegistry};
    use crate::component::Component;
    use facet::Facet;

    #[derive(Facet)]
    struct TestProperties {
        value: u32,
    }

    struct TestComponent;

    impl Component for TestComponent {
        type Properties = TestProperties;
    }

    struct OtherTestComponent;

    impl Component for OtherTestComponent {
        type Properties = TestProperties;
    }

    #[test]
    fn register_stores_components_for_later_lookup() {
        let mut registry = ComponentRegistry::default();

        let first_id = registry.register::<TestComponent>();
        let second_id = registry.register::<OtherTestComponent>();

        assert!(registry.get(first_id).is_ok());
        assert!(registry.get(second_id).is_ok());
    }

    #[test]
    fn register_overwrites_duplicate_component_types_without_changing_the_id() {
        let mut registry = ComponentRegistry::default();

        let first_id = registry.register::<TestComponent>();
        let second_id = registry.register::<TestComponent>();

        assert_eq!(first_id.0, second_id.0);
        assert!(registry.get(second_id).is_ok());
    }

    #[test]
    fn get_returns_context_for_unknown_component_ids() {
        let registry = ComponentRegistry::default();
        let error = match registry.get(ComponentId(99)) {
            Ok(_) => panic!("missing component should fail"),
            Err(error) => error,
        };

        assert!(error.to_test_string().contains("no such component: 99"));
    }
}
