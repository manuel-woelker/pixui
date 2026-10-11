#![doc = include_str!("README.md")]

//! Headless compilation. Installation is explicit and happens only on success.
use super::{
    checking::checker::check,
    diagnostic::{
        render::render,
        report::{Diagnostic, Stage},
    },
    lowering::lower::lower,
    parser::parse::parse,
    resolution::{environment::Environment, resolver::resolve},
    source::snapshot::Source,
};
use crate::application::app::Application;
use crate::ui::definition::{UiDefinition, UiDefinitionId};

pub struct CompiledDefinition {
    pub definition: UiDefinition,
    pub source: Source,
}
/// Retains immutable text so callers can format diagnostics after the file changes.
#[derive(Debug)]
pub struct CompileErrors {
    pub source: Source,
    pub diagnostics: Vec<Diagnostic>,
}
impl std::fmt::Display for CompileErrors {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for diagnostic in &self.diagnostics {
            write!(formatter, "{}", render(&self.source, diagnostic))?;
        }
        Ok(())
    }
}
impl std::error::Error for CompileErrors {}
/// Metadata must already be registered. Compilation does not create windows,
/// read resources, evaluate application data, or dispatch actions.
pub fn compile(
    filename: &str,
    text: &str,
    environment: &Environment,
) -> Result<CompiledDefinition, CompileErrors> {
    let source = Source::new(filename, text);
    let result = (|| {
        let syntax = parse(&source)?;
        let resolved = resolve(syntax, environment)?;
        let typed = check(resolved, environment)?;
        let span = typed.syntax.span;
        let definition = lower(typed, &source).map_err(|error| {
            vec![Diagnostic::new(
                Stage::Lowering,
                "C001",
                span,
                error.to_string(),
            )]
        })?;
        Ok(definition)
    })();
    match result {
        Ok(definition) => Ok(CompiledDefinition { definition, source }),
        Err(diagnostics) => Err(CompileErrors {
            source,
            diagnostics,
        }),
    }
}
impl CompiledDefinition {
    /// Embeds the compiled tree in another definition. Its messages inherit the
    /// enclosing definition's translation domain; bindings retain source origins.
    pub fn into_template(self) -> crate::live_model::part::LivePart {
        self.definition.template
    }

    /// Uses existing transactional registration, with retained source origins.
    pub fn install(self, application: &mut Application) -> pixui_base::PixuiResult<UiDefinitionId> {
        application.register_ui(self.definition)
    }
}
