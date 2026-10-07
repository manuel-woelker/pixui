//! Worker commands for language selection, catalog installation, and extraction.
use super::{app::Application, application_handle::ApplicationHandle};
use crate::{
    i18n::{
        catalog::{CatalogReport, MessageDeclaration, TranslationCatalog},
        format::TranslationFormat,
        indices::LanguageIndex,
    },
    ui::presentation::PresentationSettings,
};
use pixui_base::PixuiResult;

impl Application {
    /// Register a standalone expression for non-UI evaluation in this application's domain.
    pub fn register_expression(
        &mut self,
        domain: &str,
        expression: &crate::expression::expression::Expression,
    ) -> PixuiResult<crate::expression::expression::Expression> {
        self.translations.register_expression(domain, expression)
    }

    pub fn set_source_language(&mut self, tag: &str) -> PixuiResult<()> {
        self.translations.set_source_language(tag)
    }
    pub fn register_language(&mut self, tag: &str) -> PixuiResult<LanguageIndex> {
        self.translations.register_language(tag)
    }
    /// Compile and install atomically, then invalidate UI and native metadata.
    pub fn install_translations(
        &mut self,
        domain: &str,
        language: LanguageIndex,
        catalog: TranslationCatalog,
    ) -> PixuiResult<CatalogReport> {
        let report = self.translations.install(domain, language, catalog)?;
        self.uis.invalidate_all();
        Ok(report)
    }
    /// Set locale and indexed language together; source is selected explicitly.
    pub fn presentation_language(
        &self,
        mut settings: PresentationSettings,
        tag: &str,
    ) -> PixuiResult<PresentationSettings> {
        settings.language = self.translations.language(tag)?;
        settings.locale = self.translations.language_tag(settings.language)?.into();
        Ok(settings)
    }
}
impl ApplicationHandle {
    pub fn set_source_language(&self, tag: impl Into<String>) -> PixuiResult<()> {
        let tag = tag.into();
        self.request(move |app| app.set_source_language(&tag))?
            .wait()
    }
    pub fn register_language(&self, tag: impl Into<String>) -> PixuiResult<LanguageIndex> {
        let tag = tag.into();
        self.request(move |app| app.register_language(&tag))?.wait()
    }
    pub fn install_translations(
        &self,
        domain: impl Into<String>,
        language: LanguageIndex,
        catalog: TranslationCatalog,
    ) -> PixuiResult<CatalogReport> {
        let domain = domain.into();
        self.request(move |app| app.install_translations(&domain, language, catalog))?
            .wait()
    }
    pub fn presentation_language(
        &self,
        settings: PresentationSettings,
        tag: impl Into<String>,
    ) -> PixuiResult<PresentationSettings> {
        let tag = tag.into();
        self.inspect(move |app| app.presentation_language(settings, &tag))
    }
    pub fn translation_messages(
        &self,
        domain: impl Into<String>,
    ) -> PixuiResult<Vec<MessageDeclaration>> {
        let domain = domain.into();
        self.inspect(move |app| Ok(app.translations().declarations(&domain)))
    }
    /// Export on the caller, using a snapshot from the worker. No native UI is needed.
    pub fn export_translations(
        &self,
        domain: impl Into<String>,
        format: &dyn TranslationFormat,
    ) -> PixuiResult<String> {
        format.export(&self.translation_messages(domain)?)
    }
}
