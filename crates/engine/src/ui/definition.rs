//! Reusable named live-part templates.

use crate::live_model::part::LivePart;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiDefinitionId(pub(crate) u64);

/// Shared interaction state uses structural occurrence identities. All windows
/// of a definition share it; successful source preparation reconciles focus.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiDefinitionState {
    pub focus: Option<super::focus::ComponentInstanceId>,
    pub hover: Option<super::focus::ComponentInstanceId>,
    /// Requested logical scroll offset. Each window clamps it to its own extent.
    pub scroll: f32,
}

pub struct UiDefinition {
    pub(crate) name: String,
    pub(crate) translation_domain: String,
    pub(crate) window_expressions: Option<super::window_properties::WindowPropertyExpressions>,
    pub(crate) template: LivePart,
    pub(crate) window_properties: Option<super::window_properties::WindowPropertiesResolver>,
    pub(crate) state: UiDefinitionState,
}

impl UiDefinition {
    /// Registration validates the name. Rendering walks a private template copy
    /// so the legacy mutable walker cannot modify the shared definition.
    pub fn new(name: impl Into<String>, template: LivePart) -> Self {
        let name = name.into();
        Self {
            translation_domain: name.clone(),
            name,
            window_expressions: None,
            template,
            state: UiDefinitionState::default(),
            window_properties: None,
        }
    }

    /// Resolves native metadata on the worker, independently of visibility and
    /// painting. Without a resolver the host's startup defaults remain unchanged.
    pub fn with_window_properties(
        mut self,
        resolver: super::window_properties::WindowPropertiesResolver,
    ) -> Self {
        self.window_expressions = None;
        self.window_properties = Some(resolver);
        self
    }

    /// All messages in this definition use this domain; no subtree overrides.
    /// Defaults to the definition name. Validation happens during registration.
    pub fn with_translation_domain(mut self, domain: impl Into<String>) -> Self {
        self.translation_domain = domain.into();
        self
    }

    pub fn translation_domain(&self) -> &str {
        &self.translation_domain
    }

    pub fn with_window_property_expressions(
        mut self,
        expressions: Vec<crate::expression::expression::Expression>,
        resolve: super::window_properties::ExpressionWindowPropertiesResolver,
    ) -> Self {
        self.window_properties = None;
        self.window_expressions = Some(super::window_properties::WindowPropertyExpressions {
            expressions,
            resolve,
        });
        self
    }

    pub fn state(&self) -> &UiDefinitionState {
        &self.state
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
