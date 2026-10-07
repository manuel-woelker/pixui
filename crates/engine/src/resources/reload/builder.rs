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
    pub(crate) quiet: Duration,
}
impl ResourceReloadBuilder {
    pub fn new(application: ApplicationHandle) -> Self {
        Self {
            application,
            images: false,
            catalogs: Vec::new(),
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
        if self.catalogs.len() >= MAX_TARGETS
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
    /// Register native watches before loading. Startup errors leave no live
    /// session. Initial file errors are reported by `wait_initial`, and the
    /// session keeps watching for recovery. Call only outside the worker/UI loop.
    pub fn start(self) -> PixuiResult<ResourceReloadSession> {
        ResourceReloadSession::start(self)
    }
}
