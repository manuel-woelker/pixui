//! Worker-side registration for reusable source or Rust definitions.
use super::{app::Application, application_handle::ApplicationHandle};
use crate::{
    composition::{component_ref::ComponentRef, localized_text::LocalizedText, registry::Entry},
    expression::expression::Expression,
    live_model::part::LivePart,
    ui::definition::UiDefinition,
};
use pixui_base::{PixuiResult, pixui_error};
impl Application {
    /// Number of registered reusable definitions, excluding mounted occurrences.
    pub fn template_count(&self) -> usize {
        self.templates.len()
    }

    /// Clone a registered definition for explicit host composition. This does not
    /// mutate its registration or transfer any mounted component state.
    pub fn clone_template(&self, reference: ComponentRef) -> PixuiResult<LivePart> {
        Ok(self.templates.get(reference)?.template.as_ref().clone())
    }

    /// Registers an immutable reusable subtree and all its declarations, even
    /// while inactive. References and translation indices live until shutdown.
    pub fn register_template(
        &mut self,
        name: impl Into<String>,
        domain: &str,
        template: LivePart,
    ) -> PixuiResult<ComponentRef> {
        let name = name.into();
        if name.is_empty() {
            return Err(pixui_error!("empty component reference name"));
        }
        let mut definition =
            UiDefinition::new(name.clone(), template).with_translation_domain(domain);
        self.templates.validate(&definition.template, domain)?;
        self.components
            .validate(&definition.template, &self.painters)?;
        self.components.resolve_events(&mut definition.template)?;
        self.painters
            .validate_template_styles(&definition.template, &self.styles)?;
        let mut translations = self.translations.clone();
        crate::i18n::registration::register(&mut definition, &mut translations)?;
        let reference = self.templates.insert(Entry {
            name,
            domain: domain.into(),
            template: std::sync::Arc::new(definition.template),
        })?;
        self.translations = translations;
        Ok(reference)
    }
    /// Declare a parameterless host message through the existing i18n registry.
    /// Catalog extraction works without rendering or creating a window.
    #[track_caller]
    pub fn localized_text(&mut self, domain: &str, source: &str) -> PixuiResult<LocalizedText> {
        let expression = self
            .translations
            .register_expression(domain, &Expression::text(source)?)?;
        Ok(LocalizedText {
            expression,
            domain: domain.into(),
        })
    }
}
impl ApplicationHandle {
    pub fn register_template(
        &self,
        name: impl Into<String>,
        domain: impl Into<String>,
        template: LivePart,
    ) -> PixuiResult<ComponentRef> {
        let name = name.into();
        let domain = domain.into();
        self.request(move |app| app.register_template(name, &domain, template))?
            .wait()
    }
    pub fn localized_text(
        &self,
        domain: impl Into<String>,
        source: impl Into<String>,
    ) -> PixuiResult<LocalizedText> {
        let domain = domain.into();
        let source = source.into();
        self.request(move |app| app.localized_text(&domain, &source))?
            .wait()
    }
}
impl ApplicationHandle {
    /// Install a host-controlled source graph on the worker. The resolver must
    /// return promptly; background fetching is outside this synchronous API.
    pub fn install_bundle<
        R: crate::ui_language::source::bundle::SourceResolver + Send + 'static,
    >(
        &self,
        resolver: R,
        entries: Vec<String>,
        environment: crate::ui_language::resolution::environment::Environment,
        domain: impl Into<String>,
        limits: crate::ui_language::bundle::BundleLimits,
    ) -> PixuiResult<crate::ui_language::bundle::InstalledBundle> {
        let domain = domain.into();
        self.request(move |app| {
            crate::ui_language::bundle::install(
                app,
                &resolver,
                &entries,
                &environment,
                &domain,
                limits,
            )
        })?
        .wait()
    }
}
