use pixui_base::PixuiString;

use super::collection::Collection;

/// A named group of collections within an application.
pub struct ApplicationSlice {
    pub name: PixuiString,
    pub collections: Vec<Collection>,
}

impl ApplicationSlice {
    /// Creates a named slice with no collections.
    pub fn new(name: impl Into<PixuiString>) -> Self {
        Self {
            name: name.into(),
            collections: Vec::new(),
        }
    }
}
