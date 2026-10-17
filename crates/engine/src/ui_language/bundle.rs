//! Transactional import-graph checking and installation. The host controls sources.
use super::{
    compiler::CompileErrors,
    diagnostic::report::{Diagnostic, Stage},
    parser::parse::parse_module,
    resolution::environment::Environment,
    source::{bundle::SourceResolver, origin::Origin, snapshot::Source, span::Span},
};
use crate::{application::app::Application, composition::component_ref::ComponentRef};
use pixui_base::{PixuiResult, pixui_error};
use std::collections::{BTreeMap, HashMap};

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
    modules: BTreeMap<String, InstalledModule>,
}
#[derive(Clone)]
struct InstalledModule {
    entry: Option<ComponentRef>,
    exports: BTreeMap<String, crate::composition::definition::CompositeExport>,
}
impl InstalledBundle {
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
/// Compile each reachable module once and install all declarations atomically.
/// Modules share the host exports; aliases remain local to the importing file.
/// On any failure both component registrations and translations are restored.
pub fn install(
    application: &mut Application,
    resolver: &dyn SourceResolver,
    entries: &[String],
    environment: &Environment,
    domain: &str,
    limits: BundleLimits,
) -> PixuiResult<InstalledBundle> {
    let templates = application.templates.clone();
    let translations = application.translations.clone();
    let result = (|| {
        crate::i18n::registry::validate_domain(domain)?;
        let mut loader = Loader {
            application,
            resolver,
            environment,
            domain,
            limits,
            modules: BTreeMap::new(),
            active: vec![],
            bytes: 0,
            declarations: 0,
        };
        for entry in entries {
            loader.load(None, entry)?;
        }
        Ok(InstalledBundle {
            modules: loader.modules,
        })
    })();
    if result.is_err() {
        application.templates = templates;
        application.translations = translations;
    }
    result
}
struct Loader<'a> {
    application: &'a mut Application,
    resolver: &'a dyn SourceResolver,
    environment: &'a Environment,
    domain: &'a str,
    limits: BundleLimits,
    modules: BTreeMap<String, InstalledModule>,
    active: Vec<String>,
    bytes: usize,
    declarations: usize,
}
impl Loader<'_> {
    fn load(&mut self, importer: Option<&str>, specifier: &str) -> PixuiResult<InstalledModule> {
        let module = self.resolver.resolve(importer, specifier)?;
        if let Some(reference) = self.modules.get(&module.id) {
            return Ok(reference.clone());
        }
        if self.active.contains(&module.id) {
            return Err(pixui_error!(
                "cyclic UI imports: {} -> {}",
                self.active.join(" -> "),
                module.id
            ));
        }
        if self.active.len() >= self.limits.depth
            || self.modules.len() + self.active.len() >= self.limits.modules
        {
            return Err(pixui_error!("UI import graph limit exceeded"));
        }
        self.bytes = self
            .bytes
            .checked_add(module.text.len())
            .ok_or_else(|| pixui_error!("UI source byte limit exceeded"))?;
        if self.bytes > self.limits.source_bytes {
            return Err(pixui_error!("UI source byte limit exceeded"));
        }
        let source = Source::new(module.id.clone(), module.text);
        let syntax = parse_module(&source).map_err(|diagnostics| {
            pixui_error!(
                "{}",
                CompileErrors {
                    source: source.clone(),
                    diagnostics
                }
            )
        })?;
        self.declarations = self
            .declarations
            .checked_add(syntax.components.len() + usize::from(syntax.entry.is_some()))
            .ok_or_else(|| pixui_error!("UI declaration limit exceeded"))?;
        if self.declarations > self.limits.declarations {
            return Err(pixui_error!("UI declaration limit exceeded"));
        }
        if let Some(domain) = syntax
            .entry
            .as_ref()
            .and_then(|entry| entry.domain.as_ref())
            && domain.value != self.domain
        {
            return Err(at(
                &source,
                domain.span,
                "imported component has incompatible translation domain",
            ));
        }
        self.active.push(module.id.clone());
        let mut env = self.environment.clone();
        let mut aliases: HashMap<String, Span> = HashMap::new();
        for import in &syntax.imports {
            let dependency = self
                .load(Some(&module.id), &import.path.value)
                .map_err(|error| {
                    Origin {
                        source: source.clone(),
                        span: import.path.span,
                    }
                    .error(error)
                })?;
            let names = if import.members.is_empty() {
                vec![&import.alias]
            } else {
                import.members.iter().map(|member| &member.alias).collect()
            };
            for alias in names {
                if let Some(previous) = aliases.insert(alias.value.clone(), alias.span) {
                    return Err(pixui_error!(
                        "{}",
                        CompileErrors {
                            source: source.clone(),
                            diagnostics: vec![
                                Diagnostic::new(
                                    Stage::Resolution,
                                    "R015",
                                    alias.span,
                                    "duplicate use alias"
                                )
                                .secondary(previous, "first use")
                            ]
                        }
                    ));
                }
            }
            if import.members.is_empty() {
                let reference = dependency.entry.ok_or_else(|| {
                    at(
                        &source,
                        import.path.span,
                        "library module has no ui entry; use a named export",
                    )
                })?;
                env.template(&import.alias.value, reference)
                    .map_err(|error| at(&source, import.alias.span, &error.to_string()))?;
            } else {
                for member in &import.members {
                    let export = dependency.exports.get(&member.name.value).ok_or_else(|| {
                        at(
                            &source,
                            member.name.span,
                            &format!("unknown or private export `{}`", member.name.value),
                        )
                    })?;
                    env.composite(&member.alias.value, export)
                        .map_err(|error| at(&source, member.alias.span, &error.to_string()))?;
                }
            }
        }
        let compiled = super::checking::composites::compile_module(source, syntax, &env)
            .map_err(|error| pixui_error!("{error}"))?;
        let mut exports = BTreeMap::new();
        for (name, export) in compiled.exports {
            exports.insert(
                name,
                self.application.register_composite(self.domain, &export)?,
            );
        }
        let entry = compiled
            .entry
            .map(|entry| {
                self.application
                    .register_template(&module.id, self.domain, entry.into_template())
            })
            .transpose()?;
        let installed = InstalledModule { entry, exports };
        self.active.pop();
        self.modules.insert(module.id, installed.clone());
        Ok(installed)
    }
}
fn at(source: &Source, span: Span, message: &str) -> pixui_base::PixuiError {
    pixui_error!(
        "{}",
        CompileErrors {
            source: source.clone(),
            diagnostics: vec![Diagnostic::new(Stage::Resolution, "R014", span, message)]
        }
    )
}
