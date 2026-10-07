//! Resources rooted in an explicit native directory.
use super::{
    filesystem::{ResourceFilesystem, ResourceReader},
    path::ResourcePath,
};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    fs::{self, File},
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// A canonical directory root, independent of subsequent working-directory
/// changes. Symlinks escaping the root are rejected. This containment check is
/// not a sandbox against hostile concurrent filesystem changes (TOCTOU).
pub struct DirectoryFilesystem {
    root: PathBuf,
}
impl DirectoryFilesystem {
    pub fn new(root: impl AsRef<Path>) -> PixuiResult<Self> {
        let supplied = root.as_ref();
        let root = fs::canonicalize(supplied)
            .map_err(|error| pixui_error!("resource root {}: {error}", supplied.display()))?;
        if !root.is_dir() {
            return Err(pixui_error!(
                "resource root {} is not a directory",
                root.display()
            ));
        }
        Ok(Self { root })
    }
}
impl ResourceFilesystem for DirectoryFilesystem {
    fn open(&self, path: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
        let candidate = self.root.join(path.as_str());
        let resolved = match fs::canonicalize(&candidate) {
            Ok(path) => path,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(pixui_error!(
                    "resolve resource {}: {error}",
                    candidate.display()
                ));
            }
        };
        if !resolved.starts_with(&self.root) {
            return Err(pixui_error!(
                "resource {} escapes root {}",
                path.as_str(),
                self.root.display()
            ));
        }
        // Inspect before opening: opening a FIFO could otherwise block forever.
        if !fs::metadata(&resolved)
            .map_err(|error| pixui_error!("resource metadata {}: {error}", resolved.display()))?
            .is_file()
        {
            return Err(pixui_error!(
                "resource {} is not a regular file",
                resolved.display()
            ));
        }
        let file = File::open(&resolved)
            .map_err(|error| pixui_error!("open resource {}: {error}", resolved.display()))?;
        Ok(Some(Box::new(file)))
    }
}
