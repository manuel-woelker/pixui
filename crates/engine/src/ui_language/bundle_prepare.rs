//! Pure graph compilation. Symbolic references cannot be resolved by an application.
use super::{
    bundle::{BundleLimits, InstalledModule},
    compiler::CompileErrors,
    diagnostic::report::{Diagnostic, Stage},
    parser::parse::parse_module,
    resolution::environment::Environment,
    source::{bundle::SourceResolver, origin::Origin, snapshot::Source, span::Span},
};
use crate::{
    composition::{component_ref::ComponentRef, definition::CompositeExport},
    live_model::part::LivePart,
};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    any::TypeId,
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

pub(crate) type DefinitionKey = (String, Option<String>);
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Signature {
    pub props: Vec<(String, TypeId)>,
    pub children: bool,
}
pub(crate) struct PendingDefinition {
    pub key: DefinitionKey,
    pub body: LivePart,
    pub signature: Signature,
}
pub(crate) struct PreparedBundle {
    pub modules: BTreeMap<String, InstalledModule>,
    pub definitions: Vec<PendingDefinition>,
    pub translations: crate::i18n::registry::TranslationRegistry,
}
impl Loader<'_> {
    fn definition(
        &mut self,
        module: &str,
        name: Option<&str>,
        body: LivePart,
        signature: Signature,
    ) -> PixuiResult<ComponentRef> {
        let mut definition = crate::ui::definition::UiDefinition::new(module, body)
            .with_translation_domain(self.domain);
        crate::i18n::registration::register(&mut definition, &mut self.translations)?;
        let reference = ComponentRef {
            owner: 0,
            index: self.definitions.len(),
        };
        self.definitions.push(PendingDefinition {
            key: (module.into(), name.map(str::to_owned)),
            body: definition.template,
            signature,
        });
        Ok(reference)
    }
    fn export(
        &mut self,
        module: &str,
        name: &str,
        export: CompositeExport,
    ) -> PixuiResult<CompositeExport> {
        let signature = Signature {
            props: export
                .definition
                .props
                .iter()
                .map(|p| (p.name.clone(), p.ty.descriptor.type_id()))
                .collect(),
            children: export.definition.children,
        };
        let reference = self.definition(
            module,
            Some(name),
            export.definition.body.clone(),
            signature,
        )?;
        let mut definition = export.definition.as_ref().clone();
        definition.body = self.definitions[reference.index].body.clone();
        Ok(CompositeExport {
            definition: Arc::new(definition),
            reference: Some(reference),
        })
    }
}
pub(crate) struct Loader<'a> {
    pub translations: crate::i18n::registry::TranslationRegistry,
    pub definitions: Vec<PendingDefinition>,
    pub resolver: &'a dyn SourceResolver,
    pub environment: &'a Environment,
    pub domain: &'a str,
    pub limits: BundleLimits,
    pub modules: BTreeMap<String, InstalledModule>,
    pub active: Vec<String>,
    pub bytes: usize,
    pub declarations: usize,
}
impl Loader<'_> {
    pub fn load(
        &mut self,
        importer: Option<&str>,
        specifier: &str,
    ) -> PixuiResult<InstalledModule> {
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
            exports.insert(name.clone(), self.export(&module.id, &name, export)?);
        }
        let entry = compiled
            .entry
            .map(|entry| {
                self.definition(
                    &module.id,
                    None,
                    entry.into_template(),
                    Signature::default(),
                )
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
