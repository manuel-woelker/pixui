#![doc = include_str!("README.md")]

//! Source-independent resource lookup. Decoding belongs to consumers.
use super::path::ResourcePath;
use pixui_base::PixuiResult;
use std::io::Read;

/// An independent, owned stream. Seeking is not required; dropping closes it.
pub type ResourceReader = Box<dyn Read + Send>;

/// Read-only resources addressed by validated relative filenames.
///
/// `None` means absence, never an empty file or a pending fetch. All other
/// failures propagate. Each successful open returns an independent reader that
/// outlives a borrow of this source. Consumers must bound their reads.
/// Synchronous implementations may block; do not call from a painter/UI thread.
pub trait ResourceFilesystem: Send + Sync {
    fn open(&self, path: &ResourcePath) -> PixuiResult<Option<ResourceReader>>;

    /// Native directories mapping directly to this source's relative paths.
    /// Used only by an explicitly started reload session. Embedded/network
    /// sources return no roots. Nested layers may report overlapping roots.
    fn watch_roots(&self) -> Vec<std::path::PathBuf> {
        Vec::new()
    }
}
