#![doc = include_str!("README.md")]

//! Explicit runtime configuration. Constructing a builder starts no threads.
use super::{session::ResourceReloadSession, shared::MAX_TARGETS, target::CatalogTarget};
use crate::{
    application::application_handle::ApplicationHandle,
    i18n::{format::TranslationFormat, indices::LanguageIndex},
    resources::{filesystem::ResourceFilesystem, path::ResourcePath},
};
use pixui_base::{PixuiResult, pixui_error};
use std::{sync::Arc, time::Duration};

/// Configure one optional reload session for an application. Disabled by default:
/// ordinary applications never construct/start this builder. No Cargo features.
pub struct ResourceReloadBuilder {
    pub(crate) application: ApplicationHandle,
    pub(crate) images: bool,
    pub(crate) catalogs: Vec<CatalogTarget>,
    pub(crate) ui_bundles: Vec<super::ui_bundle::UiTarget>,
    pub(crate) quiet: Duration,
}
impl ResourceReloadBuilder {
    pub fn new(application: ApplicationHandle) -> Self {
        Self {
            application,
            images: false,
            catalogs: Vec::new(),
            ui_bundles: Vec::new(),
            quiet: Duration::from_millis(200),
        }
    }
    /// Watch the application's current image loader. Loader replacement makes
    /// its outstanding image updates stale; restart the session for a new source.
    pub fn watch_images(mut self) -> Self {
        self.images = true;
        self
    }
    /// Time without observed changes before reading. Must be in (0, 60 seconds].
    pub fn quiet_period(mut self, duration: Duration) -> Self {
        self.quiet = duration;
        self
    }
    /// Watch one explicitly registered domain/language catalog. Missing files can
    /// be created later. The adapter runs on the loader thread, never a painter.
    pub fn catalog(
        mut self,
        filesystem: Arc<dyn ResourceFilesystem>,
        path: ResourcePath,
        domain: impl Into<String>,
        language: LanguageIndex,
        format: Arc<dyn TranslationFormat + Send + Sync>,
    ) -> PixuiResult<Self> {
        let domain = domain.into();
        if domain.trim().is_empty() || language == LanguageIndex::source() {
            return Err(pixui_error!(
                "catalog needs a domain and a translation language"
            ));
        }
        if self.catalogs.len() + self.ui_bundles.len() >= MAX_TARGETS
            || self
                .catalogs
                .iter()
                .any(|target| target.domain == domain && target.language == language)
        {
            return Err(pixui_error!(
                "duplicate catalog target or too many reload targets"
            ));
        }
        self.catalogs.push(CatalogTarget {
            filesystem,
            path,
            domain,
            language,
            format,
        });
        Ok(self)
    }
    /// Reload an installed bundle from filesystem paths matching its canonical
    /// module IDs. Each accepted graph clears UI state, preserving model values.
    /// The environment is the allowlist supplied at initial installation.
    pub fn ui_bundle(
        mut self,
        filesystem: Arc<dyn ResourceFilesystem>,
        bundle: &crate::ui_language::bundle::InstalledBundle,
    ) -> PixuiResult<Self> {
        let id = bundle.id();
        if self.ui_bundles.iter().any(|target| target.bundle == id)
            || self.catalogs.len() + self.ui_bundles.len() >= MAX_TARGETS
        {
            return Err(pixui_error!(
                "duplicate UI bundle target or too many reload targets"
            ));
        }
        let entries = self
            .application
            .inspect(move |app| Ok(app.bundle_snapshot(id)?.entries))?;
        if entries.is_empty() {
            return Err(pixui_error!("UI bundle has no source entries"));
        }
        let paths = entries
            .into_iter()
            .map(ResourcePath::new)
            .collect::<PixuiResult<std::collections::BTreeSet<_>>>()?;
        self.ui_bundles.push(super::ui_bundle::UiTarget {
            filesystem,
            bundle: id,
            paths: Arc::new(std::sync::Mutex::new(paths)),
        });
        Ok(self)
    }
    /// Register native watches before loading. Startup errors leave no live
    /// session. Initial file errors are reported by `wait_initial`, and the
    /// session keeps watching for recovery. Call only outside the worker/UI loop.
    pub fn start(self) -> PixuiResult<ResourceReloadSession> {
        ResourceReloadSession::start(self)
    }
}
