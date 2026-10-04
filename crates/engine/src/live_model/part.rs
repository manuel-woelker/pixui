use pixui_base::PixuiResult;
use pixui_reflect::DynamicObject;

use super::state::GenericComponentState;
use crate::expression::expression::Expression;

/// Creates fresh owned state from the current root or loop-element context.
/// When the expression context has only application storage, receives unit.
pub type StateFactory = for<'a> fn(&DynamicObject<'a>) -> PixuiResult<GenericComponentState>;

#[derive(Clone)]
pub enum LivePart {
    Composite(CompositePart),
    Component(ComponentPart),
    ForLoop(ForLoopPart),
}

#[derive(Clone)]
pub struct CompositePart {
    pub parts: Vec<LivePart>,
}

#[derive(Clone)]
pub struct ComponentPart {
    pub create_state: StateFactory,
    /// Optional presentation callback used by the GUI renderer.
    pub presentation: Option<crate::ui::widget::WidgetFactory>,
}

impl ComponentPart {
    pub fn new(create_state: StateFactory) -> Self {
        Self {
            create_state,
            presentation: None,
        }
    }

    /// Presents a component using current data and instance settings on each walk.
    pub fn with_presentation(mut self, presentation: crate::ui::widget::WidgetFactory) -> Self {
        self.presentation = Some(presentation);
        self
    }
}

impl Default for ComponentPart {
    /// An inert component with unit state.
    fn default() -> Self {
        Self::new(|_| Ok(GenericComponentState::new(())))
    }
}

#[derive(Clone)]
pub struct ForLoopPart {
    /// Evaluated in the enclosing context; the result must be a sequence.
    pub expression: Expression,
    /// Reused for every element, with that element as the walking context.
    pub body: Box<LivePart>,
}
