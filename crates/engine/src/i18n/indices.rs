//! Application-local indices. Zero message slots are unresolved; language zero is source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MessageIndex {
    pub(crate) slot: usize,
    pub(crate) owner: u64,
}
impl MessageIndex {
    pub fn is_resolved(self) -> bool {
        self.slot != 0
    }
    pub fn get(self) -> usize {
        self.slot
    }
}

/// A language registered by one application. Default selects source text in any application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LanguageIndex {
    pub(crate) slot: usize,
    pub(crate) owner: u64,
}
impl LanguageIndex {
    pub fn source() -> Self {
        Self::default()
    }
    pub fn get(self) -> usize {
        self.slot
    }
}
