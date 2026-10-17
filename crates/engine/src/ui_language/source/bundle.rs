//! Host-controlled sources. Neither the compiler nor a mounted component opens files.
use pixui_base::{PixuiResult, pixui_error};
use std::collections::BTreeMap;
/// Canonical module identity and immutable contents returned by a source resolver.
pub struct ModuleSource {
    pub id: String,
    pub text: String,
}
/// Custom resolvers must return one stable canonical identity/content per module
/// during installation. Enforce their own filesystem/network access policy.
/// Importer is absent for entry modules; filenames in diagnostics use `id`.
pub trait SourceResolver {
    fn resolve(&self, importer: Option<&str>, specifier: &str) -> PixuiResult<ModuleSource>;
}
/// Embedded/in-memory source map. Slash-separated IDs are rooted in this bundle.
/// Relative imports may use `.` and `..`, but cannot escape the bundle root.
#[derive(Clone, Default)]
pub struct SourceBundle {
    sources: BTreeMap<String, String>,
}
impl SourceBundle {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, id: &str, source: impl Into<String>) -> PixuiResult<()> {
        let id = canonical(None, id)?;
        if self.sources.contains_key(&id) {
            return Err(pixui_error!("duplicate source module `{id}`"));
        }
        self.sources.insert(id, source.into());
        Ok(())
    }
}
impl SourceResolver for SourceBundle {
    fn resolve(&self, importer: Option<&str>, specifier: &str) -> PixuiResult<ModuleSource> {
        let id = canonical(importer, specifier)?;
        let text = self
            .sources
            .get(&id)
            .ok_or_else(|| pixui_error!("unknown source module `{id}`"))?
            .clone();
        Ok(ModuleSource { id, text })
    }
}
pub(crate) fn canonical(importer: Option<&str>, specifier: &str) -> PixuiResult<String> {
    if specifier.is_empty()
        || specifier.starts_with('/')
        || specifier.contains(['\\', ':', '\0'])
        || specifier.len() > 1024
    {
        return Err(pixui_error!("invalid relative source module path"));
    }
    let mut parts = importer
        .and_then(|id| {
            id.rsplit_once('/')
                .map(|(parent, _)| parent.split('/').collect::<Vec<_>>())
        })
        .unwrap_or_default();
    for part in specifier.split('/') {
        match part {
            "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(pixui_error!("source module path escapes bundle root"));
                }
            }
            "" => return Err(pixui_error!("empty source module path segment")),
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(pixui_error!("source module path identifies no module"));
    }
    Ok(parts.join("/"))
}
