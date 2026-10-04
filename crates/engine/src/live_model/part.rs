use pixui_base::PixuiResult;
use pixui_reflect::DynamicObject;

use super::state::GenericComponentState;

/// Creates fresh owned state from the current root or loop-element context.
pub type StateFactory = for<'a> fn(&DynamicObject<'a>) -> PixuiResult<GenericComponentState>;

pub enum LivePart {
    Composite(CompositePart),
    Component(ComponentPart),
    ForLoop(ForLoopPart),
}

pub struct CompositePart {
    pub parts: Vec<LivePart>,
}

pub struct ComponentPart {
    pub create_state: StateFactory,
}

impl ComponentPart {
    pub fn new(create_state: StateFactory) -> Self {
        Self { create_state }
    }
}

impl Default for ComponentPart {
    /// An inert component with unit state.
    fn default() -> Self {
        Self::new(|_| Ok(GenericComponentState::new(())))
    }
}

pub struct ForLoopPart {
    /// Index of a reflected sequence field in the current context descriptor.
    pub field_index: usize,
    /// Reused for every element, with that element as the walking context.
    pub body: Box<LivePart>,
}
