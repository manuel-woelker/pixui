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
/// Same-kind template replacement does not automatically reset component payloads.
#[derive(Default)]
pub enum PartState {
    #[default]
    Unknown,
    Component(ComponentState),
    Composite(CompositeState),
    ForLoop(ForLoopState),
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
/// Positions are not stable item identities when a sequence is reordered.
#[derive(Default)]
pub struct ForLoopState {
    pub items: Vec<PartState>,
}

/// Owned sendable payload so live state can move to the application worker.
pub struct GenericComponentState {
    state: Box<dyn Any + Send>,
}

impl GenericComponentState {
    /// Owns component state while erasing its concrete type.
    pub fn new(state: impl Any + Send) -> Self {
        Self {
            state: Box::new(state),
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
