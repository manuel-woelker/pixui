use std::any::Any;

/// Persistent physical state for one live-part tree.
#[derive(Default)]
pub struct LiveState {
    root_state: PartState,
}

impl LiveState {
    /// Starts with an unknown root; walking initializes reached nodes on demand.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn root_state(&self) -> &PartState {
        &self.root_state
    }

    pub fn root_state_mut(&mut self) -> &mut PartState {
        &mut self.root_state
    }
}

/// State mirrors the template, with one independent body state per loop item.
/// Children are matched by position. Reset an entry to Unknown to reinitialize it.
/// Changing registered component identity reinitializes its default state.
/// Legacy same-kind factory replacement does not reset payloads automatically.
#[derive(Default)]
pub enum PartState {
    #[default]
    Unknown,
    Component(ComponentState),
    Composite(CompositeState),
    Container(CompositeState),
    ForLoop(ForLoopState),
    Match(MatchState),
}

/// Only the selected candidate's subtree is retained. New selections begin
/// Unknown; switching or losing selection drops the old subtree. In the match
/// node visitor, selected reflects the previous walk; selection follows visit.
#[derive(Default)]
pub struct MatchState {
    pub selected: Option<usize>,
    pub part: Box<PartState>,
}

/// Owned component payload, initialized once and retained across walks.
pub struct ComponentState {
    pub state: GenericComponentState,
}

/// One state entry per composite child, in template order.
#[derive(Default)]
pub struct CompositeState {
    pub parts: Vec<PartState>,
}

/// One state entry for the complete loop body per live sequence element.
/// New entries start Unknown; removing entries drops their owned state.
/// Keys retain item identity on reorder; unkeyed loops keep positional state.
#[derive(Default)]
pub struct ForLoopState {
    /// Some for keyed loops, in the same order as items.
    pub keys: Option<Vec<super::identity::ItemKey>>,
    pub items: Vec<PartState>,
}

/// Owned sendable payload so live state can move to the application worker.
pub struct GenericComponentState {
    state: Box<dyn Any + Send>,
    component: Option<crate::component_registry::component_id::ComponentAddress>,
}

impl GenericComponentState {
    pub(crate) fn component_address(
        &self,
    ) -> Option<crate::component_registry::component_id::ComponentAddress> {
        self.component
    }

    pub(crate) fn with_component(
        mut self,
        address: crate::component_registry::component_id::ComponentAddress,
    ) -> Self {
        self.component = Some(address);
        self
    }

    /// Owns component state while erasing its concrete type.
    pub fn new(state: impl Any + Send) -> Self {
        Self {
            state: Box::new(state),
            component: None,
        }
    }

    /// Returns the state when the requested concrete type matches.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.state.downcast_ref()
    }

    /// Returns exclusive state access when the requested concrete type matches.
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.state.downcast_mut()
    }
}

impl LiveState {
    /// Resolves a published structural path without reevaluating loop/match
    /// expressions. Callers must first validate the source instance's revision.
    pub(crate) fn component_mut(
        &mut self,
        path: &super::identity::ComponentPath,
    ) -> pixui_base::PixuiResult<&mut GenericComponentState> {
        use super::identity::PathSegment;
        let mut part = &mut self.root_state;
        for segment in path.segments() {
            part = match (part, segment) {
                (
                    PartState::Composite(state) | PartState::Container(state),
                    PathSegment::Child(index),
                ) => state.parts.get_mut(*index),
                (PartState::ForLoop(state), PathSegment::LoopItem(index)) => {
                    state.items.get_mut(*index)
                }
                (PartState::ForLoop(state), PathSegment::LoopKey(key)) => {
                    let index = state
                        .keys
                        .as_ref()
                        .and_then(|keys| keys.iter().position(|candidate| candidate == key));
                    index.and_then(|index| state.items.get_mut(index))
                }
                (PartState::Match(state), PathSegment::MatchArm(index))
                    if state.selected == Some(*index) =>
                {
                    Some(state.part.as_mut())
                }
                _ => None,
            }
            .ok_or_else(|| pixui_base::pixui_error!("input target no longer exists"))?;
        }
        match part {
            PartState::Component(state) => Ok(&mut state.state),
            _ => Err(pixui_base::pixui_error!(
                "input path does not identify a component"
            )),
        }
    }
}
