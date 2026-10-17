//! Transactional source bundle installation and stable application-local references.
use super::{resolution::environment::Environment, source::bundle::SourceResolver};
use crate::{application::app::Application, composition::component_ref::ComponentRef};
use pixui_base::{PixuiResult, pixui_error};
use std::collections::BTreeMap;
/// Bounds apply to one installation, including all entry modules and dependencies.
#[derive(Clone, Copy)]
pub struct BundleLimits {
    pub modules: usize,
    pub depth: usize,
    pub source_bytes: usize,
    pub declarations: usize,
}
impl Default for BundleLimits {
    fn default() -> Self {
        Self {
            modules: 256,
            depth: 64,
            source_bytes: 8 * 1024 * 1024,
            declarations: 4096,
        }
    }
}
/// Canonical module IDs map to application-local references. Reusing this result
/// avoids recompilation and duplicate registry entries on every render/action.
#[derive(Clone)]
pub struct InstalledBundle {
    pub(crate) modules: BTreeMap<String, InstalledModule>,
    pub(crate) id: super::bundle_registration::BundleId,
}
#[derive(Clone)]
pub(crate) struct InstalledModule {
    pub entry: Option<ComponentRef>,
    pub exports: BTreeMap<String, crate::composition::definition::CompositeExport>,
}
impl InstalledBundle {
    pub fn id(&self) -> super::bundle_registration::BundleId {
        self.id
    }
    pub fn component(&self, id: &str) -> PixuiResult<ComponentRef> {
        self.modules
            .get(id)
            .ok_or_else(|| pixui_error!("module `{id}` was not installed"))?
            .entry
            .ok_or_else(|| {
                pixui_error!("library module `{id}` has no ui entry; select a named export")
            })
    }
    /// Select an explicit named export, including its checked prop/slot signature.
    pub fn export(
        &self,
        id: &str,
        name: &str,
    ) -> PixuiResult<crate::composition::definition::CompositeExport> {
        self.modules
            .get(id)
            .and_then(|module| module.exports.get(name))
            .cloned()
            .ok_or_else(|| pixui_error!("unknown or private export `{name}` in module `{id}`"))
    }
    pub fn len(&self) -> usize {
        self.modules.len()
    }
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}
/// Compile and install a source graph atomically. Preparation is also used by
/// the background resource loader; only linking/registration mutates application state.
pub fn install(
    application: &mut Application,
    resolver: &dyn SourceResolver,
    entries: &[String],
    environment: &Environment,
    domain: &str,
    limits: BundleLimits,
) -> PixuiResult<InstalledBundle> {
    let prepared = prepare(
        resolver,
        entries,
        environment,
        domain,
        limits,
        application.translations.clone(),
    )?;
    super::bundle_registration::install(
        application,
        prepared,
        entries.to_vec(),
        environment.clone(),
        domain.into(),
        limits,
        None,
    )
}
pub(crate) fn prepare(
    resolver: &dyn SourceResolver,
    entries: &[String],
    environment: &Environment,
    domain: &str,
    limits: BundleLimits,
    translations: crate::i18n::registry::TranslationRegistry,
) -> PixuiResult<super::bundle_prepare::PreparedBundle> {
    crate::i18n::registry::validate_domain(domain)?;
    let mut loader = super::bundle_prepare::Loader {
        resolver,
        environment,
        domain,
        limits,
        translations,
        definitions: vec![],
        modules: BTreeMap::new(),
        active: vec![],
        bytes: 0,
        declarations: 0,
    };
    for entry in entries {
        loader.load(None, entry)?;
    }
    Ok(super::bundle_prepare::PreparedBundle {
        modules: loader.modules,
        definitions: loader.definitions,
        translations: loader.translations,
    })
}
