//! Shared bounded reads for resource consumers, including background reloads.
use super::{filesystem::ResourceFilesystem, path::ResourcePath};
use pixui_base::{PixuiResult, pixui_error};
use std::io::Read;

pub(crate) fn read_bounded(
    filesystem: &dyn ResourceFilesystem,
    path: &ResourcePath,
    limit: usize,
) -> PixuiResult<Vec<u8>> {
    let bound = limit
        .checked_add(1)
        .ok_or_else(|| pixui_error!("invalid resource read limit"))?;
    let reader = filesystem
        .open(path)?
        .ok_or_else(|| pixui_error!("resource `{}` not found", path.as_str()))?;
    let mut bytes = Vec::new();
    reader
        .take(bound as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| pixui_error!("read resource `{}`: {error}", path.as_str()))?;
    if bytes.len() > limit {
        return Err(pixui_error!(
            "resource `{}` exceeds encoded byte limit",
            path.as_str()
        ));
    }
    Ok(bytes)
}
