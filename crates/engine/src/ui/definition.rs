//! Reusable named live-part templates.

use crate::live_model::part::LivePart;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiDefinitionId(pub(crate) u64);

/// Shared interaction state. Component indices refer to prepared component order,
/// including noninteractive components. All windows of a definition share it.
/// Content changes clear positional focus/hover. The most recent input wins.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiDefinitionState {
    pub focus: Option<usize>,
    pub hover: Option<usize>,
    /// Requested logical scroll offset. Each window clamps it to its own extent.
    pub scroll: f32,
}

pub struct UiDefinition {
    pub(crate) name: String,
    pub(crate) template: LivePart,
    pub(crate) state: UiDefinitionState,
}

impl UiDefinition {
    /// Registration validates the name. Rendering walks a private template copy
    /// so the legacy mutable walker cannot modify the shared definition.
    pub fn new(name: impl Into<String>, template: LivePart) -> Self {
        Self {
            name: name.into(),
            template,
            state: UiDefinitionState::default(),
        }
    }

    pub fn state(&self) -> &UiDefinitionState {
        &self.state
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
