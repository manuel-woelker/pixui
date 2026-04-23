/// Identifier for a style in the draw command stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StyleId(usize);

impl StyleId {
    /// Creates a new style identifier.
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    /// Returns the raw style index.
    pub fn index(self) -> usize {
        self.0
    }
}
