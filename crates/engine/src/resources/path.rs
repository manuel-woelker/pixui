//! Portable filenames within a resource filesystem.
use pixui_base::{PixuiResult, PixuiString, pixui_error};

/// A nonempty, slash-separated relative filename with one canonical spelling.
/// Absolute paths, empty/dot/parent components, backslashes, colons and NULs are
/// rejected. Case is preserved; use exact case for portability across sources.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResourcePath(PixuiString);

impl ResourcePath {
    pub fn new(value: impl Into<PixuiString>) -> PixuiResult<Self> {
        let value = value.into();
        if value.contains(['\\', ':', '\0'])
            || value
                .as_str()
                .split('/')
                .any(|part| matches!(part, "" | "." | ".."))
        {
            return Err(pixui_error!("invalid relative resource filename `{value}`"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
