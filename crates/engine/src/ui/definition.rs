//! Reusable named live-part templates.

use crate::live_model::part::LivePart;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiDefinitionId(pub(crate) u64);

pub struct UiDefinition {
    pub(crate) name: String,
    pub(crate) template: LivePart,
}

impl UiDefinition {
    /// Registration validates the name. Rendering walks a private template copy
    /// so the legacy mutable walker cannot modify the shared definition.
    pub fn new(name: impl Into<String>, template: LivePart) -> Self {
        Self {
            name: name.into(),
            template,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
