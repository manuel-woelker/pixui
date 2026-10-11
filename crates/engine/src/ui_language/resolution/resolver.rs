//! Lexical scopes are resolved to de Bruijn depths (zero is the innermost loop).
use super::{
    environment::Environment,
    resolved::{
        self, Expression, ExpressionKind, PartKind, ResolvedDefinition, ResolvedPart, Root, Segment,
    },
};
use crate::ui_language::{
    ast::{
        definition::Definition,
        expression::{ExpressionKind as AstExpression, SegmentKind},
        part::{Part, PartKind as AstPart},
    },
    diagnostic::report::{Diagnostic, Stage},
    lexer::scan::MAX_DIAGNOSTICS,
};

pub fn resolve(
    definition: Definition,
    environment: &Environment,
) -> Result<ResolvedDefinition, Vec<Diagnostic>> {
    let mut resolver = Resolver {
        environment,
        locals: vec![],
        errors: vec![],
    };
    let root = resolver.part(&definition.root);
    match root {
        Ok(root) if resolver.errors.is_empty() => Ok(ResolvedDefinition {
            syntax: definition,
            root,
        }),
        Err(error) => {
            if resolver.errors.len() < MAX_DIAGNOSTICS {
                resolver.errors.push(error);
            }
            Err(resolver.errors)
        }
        _ => Err(resolver.errors),
    }
}
struct Resolver<'a> {
    environment: &'a Environment,
    locals: Vec<String>,
    errors: Vec<Diagnostic>,
}
impl Resolver<'_> {
    fn part(&mut self, part: &Part) -> Result<ResolvedPart, Diagnostic> {
        let kind = match &part.kind {
            AstPart::Element {
                name,
                attributes,
                children,
            } => {
                let component = match name {
                    Some(name) if !matches!(name.value.as_str(), "Row" | "Column" | "Grid") => {
                        Some(
                            self.environment
                                .components
                                .get(&name.value)
                                .cloned()
                                .ok_or_else(|| {
                                    Diagnostic::new(
                                        Stage::Resolution,
                                        "R001",
                                        name.span,
                                        format!("unknown component `{}`", name.value),
                                    )
                                })?,
                        )
                    }
                    _ => None,
                };
                let mut seen = std::collections::HashMap::new();
                let mut resolved = vec![];
                for attribute in attributes {
                    if let Some(previous) = seen.insert(&attribute.name.value, attribute.name.span)
                    {
                        return Err(Diagnostic::new(
                            Stage::Resolution,
                            "R002",
                            attribute.name.span,
                            "duplicate attribute",
                        )
                        .secondary(previous, "first attribute"));
                    }
                    if let Some(component) = &component
                        && !matches!(
                            attribute.name.value.as_str(),
                            "on_activate" | "layout" | "styles"
                        )
                        && !component
                            .props
                            .fields()
                            .iter()
                            .any(|field| field.name == attribute.name.value)
                    {
                        return Err(Diagnostic::new(
                            Stage::Resolution,
                            "R003",
                            attribute.name.span,
                            format!("unknown property `{}`", attribute.name.value),
                        ));
                    }
                    resolved.push((attribute.name.clone(), self.expression(&attribute.value)?));
                }
                let mut parts = vec![];
                for child in children {
                    match self.part(child) {
                        Ok(part) => parts.push(part),
                        Err(error) if self.errors.len() < MAX_DIAGNOSTICS => {
                            self.errors.push(error)
                        }
                        Err(_) => break,
                    }
                }
                PartKind::Element {
                    component,
                    attributes: resolved,
                    children: parts,
                }
            }
            AstPart::For {
                binding,
                sequence,
                body,
            } => {
                let sequence = self.expression(sequence)?;
                self.locals.push(binding.value.clone());
                let body = self.part(body);
                self.locals.pop();
                PartKind::For {
                    sequence,
                    body: Box::new(body?),
                }
            }
            AstPart::Match { value, arms } => PartKind::Match {
                value: self.expression(value)?,
                arms: arms
                    .iter()
                    .map(|arm| {
                        Ok((
                            arm.pattern
                                .as_ref()
                                .map(|pattern| self.expression(pattern))
                                .transpose()?,
                            self.part(&arm.body)?,
                            arm.span,
                        ))
                    })
                    .collect::<Result<_, Diagnostic>>()?,
            },
        };
        Ok(ResolvedPart {
            syntax: part.clone(),
            kind,
        })
    }
    fn expression(
        &self,
        expression: &crate::ui_language::ast::expression::Expression,
    ) -> Result<Expression, Diagnostic> {
        let kind = match &expression.kind {
            AstExpression::Bool(value) => ExpressionKind::Bool(*value),
            AstExpression::Number(value) => ExpressionKind::Number(value.clone()),
            AstExpression::Template {
                translated,
                segments,
            } => ExpressionKind::Template {
                translated: *translated,
                segments: segments
                    .iter()
                    .map(|segment| match &segment.kind {
                        SegmentKind::Text(text) => Ok(Segment::Text(text.clone(), segment.span)),
                        SegmentKind::Value(value) => self.expression(value).map(Segment::Value),
                    })
                    .collect::<Result<_, _>>()?,
            },
            AstExpression::Path(path) => {
                let first = &path[0];
                if matches!(first.value.as_str(), "layouts" | "styles") {
                    let name = path.get(1).filter(|_| path.len() == 2).ok_or_else(|| {
                        Diagnostic::new(
                            Stage::Resolution,
                            "R010",
                            expression.span,
                            "expected exported layout/style name",
                        )
                    })?;
                    let root = if first.value == "layouts" {
                        Root::Layout(Box::new(
                            self.environment
                                .layouts
                                .get(&name.value)
                                .cloned()
                                .ok_or_else(|| {
                                    Diagnostic::new(
                                        Stage::Resolution,
                                        "R010",
                                        name.span,
                                        "unknown layout export",
                                    )
                                })?,
                        ))
                    } else {
                        Root::Styles(
                            self.environment
                                .styles
                                .get(&name.value)
                                .cloned()
                                .ok_or_else(|| {
                                    Diagnostic::new(
                                        Stage::Resolution,
                                        "R010",
                                        name.span,
                                        "unknown styles export",
                                    )
                                })?,
                        )
                    };
                    return Ok(Expression {
                        spelling: None,
                        span: expression.span,
                        kind: ExpressionKind::Path {
                            root,
                            fields: vec![],
                        },
                    });
                }

                let (root, skip) = if let Some(depth) = self
                    .locals
                    .iter()
                    .rev()
                    .position(|name| name == &first.value)
                {
                    (Root::Local(depth), 1)
                } else {
                    let map = match first.value.as_str() {
                        "entities" => &self.environment.entities,
                        "collections" => &self.environment.collections,
                        _ => {
                            return Err(Diagnostic::new(
                                Stage::Resolution,
                                "R004",
                                first.span,
                                format!("unknown name `{}`", first.value),
                            ));
                        }
                    };
                    let name = path.get(1).ok_or_else(|| {
                        Diagnostic::new(
                            Stage::Resolution,
                            "R005",
                            first.span,
                            "namespace requires an exported name",
                        )
                    })?;
                    (
                        Root::Data(Box::new(map.get(&name.value).cloned().ok_or_else(
                            || {
                                Diagnostic::new(
                                    Stage::Resolution,
                                    "R006",
                                    name.span,
                                    format!("unknown exported name `{}`", name.value),
                                )
                            },
                        )?)),
                        2,
                    )
                };
                ExpressionKind::Path {
                    root,
                    fields: path[skip..].to_vec(),
                }
            }
            AstExpression::Call { path, arguments } => {
                if path.len() != 2 || path[0].value != "actions" {
                    return Err(Diagnostic::new(
                        Stage::Resolution,
                        "R007",
                        expression.span,
                        "only exported action calls are supported",
                    ));
                }
                let handle = *self
                    .environment
                    .actions
                    .get(&path[1].value)
                    .ok_or_else(|| {
                        Diagnostic::new(Stage::Resolution, "R008", path[1].span, "unknown action")
                    })?;
                ExpressionKind::Call {
                    handle,
                    arguments: arguments
                        .iter()
                        .map(|argument| self.expression(argument))
                        .collect::<Result<_, _>>()?,
                }
            }
            AstExpression::Not(value) => ExpressionKind::Not(Box::new(self.expression(value)?)),
            AstExpression::Binary { op, left, right } => ExpressionKind::Binary {
                op: *op,
                left: Box::new(self.expression(left)?),
                right: Box::new(self.expression(right)?),
            },
        };
        Ok(resolved::Expression {
            spelling: match &expression.kind {
                AstExpression::Path(path) => Some(
                    path.iter()
                        .map(|name| name.value.as_str())
                        .collect::<Vec<_>>()
                        .join("."),
                ),
                _ => None,
            },
            kind,
            span: expression.span,
        })
    }
}
