//! Focus eligibility and definition-scoped occurrence handles.

use super::definition::UiDefinitionId;
use crate::live_model::identity::ComponentPath;

/// Focus is independent of activation. Automatic keeps ordinary buttons
/// sequentially focusable; inert components default to no focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusBehavior {
    #[default]
    Automatic,
    Sequential,
    /// Pointer and explicit requests can focus this target; Tab skips it.
    Direct,
    None,
}

impl FocusBehavior {
    pub(crate) fn resolved(self, activatable: bool) -> Self {
        match self {
            Self::Automatic if activatable => Self::Sequential,
            Self::Automatic => Self::None,
            other => other,
        }
    }
}

/// Resolved each render, allowing application data to change eligibility.
pub type FocusResolver = for<'a> fn(
    &crate::expression::context::ExpressionContext<'a>,
    &super::presentation::PresentationSettings,
) -> pixui_base::PixuiResult<FocusBehavior>;

/// An occurrence within one registered definition, not a registered component
/// type. Obtain from a UiInstance's published layout; IDs contain no geometry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ComponentInstanceId {
    pub(crate) definition: UiDefinitionId,
    pub(crate) path: ComponentPath,
}

impl ComponentInstanceId {
    pub fn definition(&self) -> UiDefinitionId {
        self.definition
    }
    pub fn path(&self) -> &ComponentPath {
        &self.path
    }
}
