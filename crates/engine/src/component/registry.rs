use crate::component::Component;
use pixui_base::bail;
use pixui_base::result::{OptionExt, PixuiResult};
use std::any::{TypeId, type_name};
use std::collections::HashMap;

#[derive(Debug, Copy, Clone)]
pub struct ComponentId(usize);

pub struct ComponentHolder {}

#[derive(Default)]
pub struct ComponentRegistry {
    component_map: HashMap<TypeId, ComponentId>,
    components: Vec<ComponentHolder>,
}

impl ComponentRegistry {
    pub fn register<C: Component + 'static>(&mut self) -> PixuiResult<ComponentId> {
        let type_id = TypeId::of::<C>();
        if self.component_map.contains_key(&type_id) {
            bail!("Component already registered: {:?}", type_name::<C>());
        }
        let component_id = ComponentId(self.components.len());
        self.components.push(ComponentHolder {});
        self.component_map.insert(type_id, component_id);
        Ok(component_id)
    }

    pub fn get(&self, id: ComponentId) -> PixuiResult<&ComponentHolder> {
        self.components
            .get(id.0)
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

        let first_id = registry.register::<TestComponent>().unwrap();
        let second_id = registry.register::<OtherTestComponent>().unwrap();

        assert!(registry.get(first_id).is_ok());
        assert!(registry.get(second_id).is_ok());
    }

    #[test]
    fn register_rejects_duplicate_component_types() {
        let mut registry = ComponentRegistry::default();

        registry.register::<TestComponent>().unwrap();
        let error = registry.register::<TestComponent>().unwrap_err();
        let rendered = error.to_test_string();

        assert!(rendered.contains("Component already registered"));
        assert!(rendered.contains("TestComponent"));
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
