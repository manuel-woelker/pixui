//! Local declarations form a checked DAG; bodies retain declaration-site scope.
use crate::{
    composition::definition::{CompositeDefinition, CompositeExport, Prop},
    ui_language::{
        ast::{
            definition::{Component, Definition, Module},
            part::{Part, PartKind},
        },
        compiler::{CompileErrors, CompiledDefinition, compile_syntax},
        diagnostic::report::{Diagnostic, Stage},
        resolution::environment::Environment,
        source::{origin::Origin, snapshot::Source},
    },
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

pub struct CompiledModule {
    pub entry: Option<CompiledDefinition>,
    pub exports: BTreeMap<String, CompositeExport>,
}
/// Caller has resolved external uses into `environment`; local declarations stay
/// private unless explicitly exported. All declarations are checked, even unused.
pub(crate) fn compile_module(
    source: Source,
    module: Module,
    environment: &Environment,
) -> Result<CompiledModule, CompileErrors> {
    let mut env = environment.clone();
    let mut names = HashMap::new();
    for (index, declaration) in module.components.iter().enumerate() {
        let name = &declaration.name;
        if let Some(previous) = names.insert(name.value.clone(), index) {
            return Err(error(
                &source,
                Diagnostic::new(
                    Stage::Resolution,
                    "R018",
                    name.span,
                    "duplicate component declaration",
                )
                .secondary(module.components[previous].name.span, "first declaration"),
            ));
        }
        if matches!(
            name.value.as_str(),
            "Row" | "Column" | "Grid" | "Dynamic" | "Children"
        ) || env.components.contains_key(&name.value)
            || env.templates.contains_key(&name.value)
            || env.composites.contains_key(&name.value)
            || module
                .entry
                .as_ref()
                .is_some_and(|entry| entry.name.value == name.value)
        {
            return Err(error(
                &source,
                Diagnostic::new(
                    Stage::Resolution,
                    "R018",
                    name.span,
                    "reserved or conflicting component name",
                ),
            ));
        }
    }
    let mut builder = Builder {
        source: &source,
        declarations: &module.components,
        names,
        active: vec![],
        done: HashSet::new(),
        env: &mut env,
    };
    for index in 0..module.components.len() {
        builder.compile(index)?;
    }
    let exports = module
        .components
        .iter()
        .filter(|component| component.export.is_some())
        .map(|component| {
            (
                component.name.value.clone(),
                CompositeExport {
                    definition: env.composites[&component.name.value].clone(),
                    reference: None,
                },
            )
        })
        .collect();
    // The module's ui entry cannot access a declaration's props or slot frame.
    env.props.clear();
    env.children_slot = false;
    let entry = module
        .entry
        .map(|syntax| compile_syntax(source.clone(), syntax, &env))
        .transpose()?;
    Ok(CompiledModule { entry, exports })
}
struct Builder<'a> {
    source: &'a Source,
    declarations: &'a [Component],
    names: HashMap<String, usize>,
    active: Vec<usize>,
    done: HashSet<usize>,
    env: &'a mut Environment,
}
impl Builder<'_> {
    fn compile(&mut self, index: usize) -> Result<(), CompileErrors> {
        if self.done.contains(&index) {
            return Ok(());
        }
        let declaration = &self.declarations[index];
        if self.active.len() >= 64 || self.active.contains(&index) {
            return Err(error(
                self.source,
                Diagnostic::new(
                    Stage::Resolution,
                    "R019",
                    declaration.name.span,
                    format!(
                        "cyclic or excessively nested component declarations: {} -> {}",
                        self.active
                            .iter()
                            .map(|index| self.declarations[*index].name.value.as_str())
                            .collect::<Vec<_>>()
                            .join(" -> "),
                        declaration.name.value
                    ),
                ),
            ));
        }
        self.active.push(index);
        let mut dependencies = vec![];
        let mut slots = 0;
        let mut pending = vec![&declaration.body];
        while let Some(part) = pending.pop() {
            if let PartKind::Element {
                name: Some(name), ..
            } = &part.kind
            {
                if name.value == "Children" {
                    slots += 1;
                }
                if let Some(dependency) = self.names.get(&name.value) {
                    dependencies.push(*dependency);
                }
            }
            descend(part, &mut pending);
        }
        if slots > 1 {
            return Err(error(
                self.source,
                Diagnostic::new(
                    Stage::Checking,
                    "T005",
                    declaration.span,
                    "component may declare at most one Children placeholder",
                ),
            ));
        }
        for dependency in dependencies {
            self.compile(dependency)?;
        }
        let mut env = self.env.clone();
        env.props.clear();
        env.children_slot = false;
        let mut names = HashMap::new();
        for parameter in &declaration.parameters {
            if let Some(previous) = names.insert(parameter.name.value.clone(), parameter.name.span)
            {
                return Err(error(
                    self.source,
                    Diagnostic::new(
                        Stage::Checking,
                        "T005",
                        parameter.name.span,
                        "duplicate component parameter",
                    )
                    .secondary(previous, "first parameter"),
                ));
            }
            if parameter.ty.value == "Children" {
                if parameter.name.value != "children" {
                    return Err(error(
                        self.source,
                        Diagnostic::new(
                            Stage::Checking,
                            "T005",
                            parameter.name.span,
                            "the Children slot parameter must be named children",
                        ),
                    ));
                }
                env.children_slot = true;
            } else {
                if parameter.name.value == "children" {
                    return Err(error(
                        self.source,
                        Diagnostic::new(
                            Stage::Checking,
                            "T005",
                            parameter.name.span,
                            "children is reserved for the Children slot",
                        ),
                    ));
                }
                let ty = env
                    .prop_types
                    .get(&parameter.ty.value)
                    .cloned()
                    .ok_or_else(|| {
                        error(
                            self.source,
                            Diagnostic::new(
                                Stage::Checking,
                                "T005",
                                parameter.ty.span,
                                format!(
                                    "unknown prop type `{}`; export a type alias explicitly",
                                    parameter.ty.value
                                ),
                            ),
                        )
                    })?;
                env.props.push(Prop {
                    name: parameter.name.value.clone(),
                    ty,
                    origin: Origin {
                        source: self.source.clone(),
                        span: parameter.span,
                    },
                });
            }
        }
        let syntax = Definition {
            imports: vec![],
            name: declaration.name.clone(),
            domain: None,
            root: declaration.body.clone(),
            span: declaration.span,
        };
        let compiled = compile_syntax(self.source.clone(), syntax, &env)?;
        let definition = Arc::new(CompositeDefinition {
            id: CompositeDefinition::next_id(),
            name: declaration.name.value.clone(),
            props: env.props,
            children: env.children_slot,
            body: compiled.into_template(),
            origin: Origin {
                source: self.source.clone(),
                span: declaration.name.span,
            },
        });
        self.env
            .composites
            .insert(declaration.name.value.clone(), definition);
        self.active.pop();
        self.done.insert(index);
        Ok(())
    }
}
fn descend<'a>(part: &'a Part, pending: &mut Vec<&'a Part>) {
    match &part.kind {
        PartKind::Element { children, .. } => pending.extend(children),
        PartKind::For { body, .. } => pending.push(body),
        PartKind::Match { arms, .. } => pending.extend(arms.iter().map(|arm| &arm.body)),
    }
}
fn error(source: &Source, diagnostic: Diagnostic) -> CompileErrors {
    CompileErrors {
        source: source.clone(),
        diagnostics: vec![diagnostic],
    }
}
