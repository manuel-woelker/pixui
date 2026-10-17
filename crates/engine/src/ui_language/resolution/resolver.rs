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
        slice: None,
        event: None,
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
    slice: Option<String>,
    event: Option<&'static pixui_reflect::TypeDescriptor>,
}
impl Resolver<'_> {
    fn part(&mut self, part: &Part) -> Result<ResolvedPart, Diagnostic> {
        let previous = self.slice.clone();
        let result = self.part_inner(part);
        self.slice = previous;
        self.event = None;
        result
    }
    fn part_inner(&mut self, part: &Part) -> Result<ResolvedPart, Diagnostic> {
        let kind = match &part.kind {
            AstPart::Element {
                name,
                attributes,
                children,
            } => {
                if let Some(name) = name {
                    if name.value == "Children" {
                        if !self.environment.children_slot
                            || !attributes.is_empty()
                            || !children.is_empty()
                        {
                            return Err(Diagnostic::new(
                                Stage::Resolution,
                                "R016",
                                part.span,
                                "Children requires a declared children: Children slot and accepts no attributes or children",
                            ));
                        }
                        return Ok(ResolvedPart {
                            syntax: part.clone(),
                            kind: PartKind::Slot,
                        });
                    }
                    if let Some(definition) = self.environment.composites.get(&name.value).cloned()
                    {
                        if !definition.children && !children.is_empty() {
                            return Err(Diagnostic::new(
                                Stage::Resolution,
                                "R016",
                                part.span,
                                "composite does not declare a child slot",
                            )
                            .declaration(&definition.origin, "component declared here"));
                        }
                        let mut seen = std::collections::HashMap::new();
                        let mut values = vec![];
                        for attribute in attributes {
                            if let Some(previous) =
                                seen.insert(attribute.name.value.clone(), attribute.name.span)
                            {
                                return Err(Diagnostic::new(
                                    Stage::Resolution,
                                    "R002",
                                    attribute.name.span,
                                    "duplicate composite prop",
                                )
                                .secondary(previous, "first prop"));
                            }
                            if !definition
                                .props
                                .iter()
                                .any(|prop| prop.name == attribute.name.value)
                            {
                                return Err(Diagnostic::new(
                                    Stage::Resolution,
                                    "R016",
                                    attribute.name.span,
                                    format!(
                                        "unknown prop `{}` on composite `{}`",
                                        attribute.name.value, definition.name
                                    ),
                                )
                                .declaration(&definition.origin, "component declared here"));
                            }
                            values
                                .push((attribute.name.clone(), self.expression(&attribute.value)?));
                        }
                        let children = children
                            .iter()
                            .map(|child| self.part(child))
                            .collect::<Result<Vec<_>, _>>()?;
                        return Ok(ResolvedPart {
                            syntax: part.clone(),
                            kind: PartKind::Invoke {
                                definition,
                                attributes: values,
                                children,
                            },
                        });
                    }
                }
                if let Some(name) = name
                    && (name.value == "Dynamic"
                        || self.environment.templates.contains_key(&name.value))
                {
                    if !children.is_empty() {
                        return Err(Diagnostic::new(
                            Stage::Resolution,
                            "R013",
                            part.span,
                            "composed components do not accept children",
                        ));
                    }
                    let expression = if name.value == "Dynamic" {
                        if attributes.len() != 1 || attributes[0].name.value != "component" {
                            return Err(Diagnostic::new(
                                Stage::Resolution,
                                "R013",
                                part.span,
                                "Dynamic requires exactly one component attribute",
                            ));
                        }
                        self.expression(&attributes[0].value)?
                    } else {
                        if !attributes.is_empty() {
                            return Err(Diagnostic::new(
                                Stage::Resolution,
                                "R013",
                                part.span,
                                "composed components do not accept attributes yet",
                            ));
                        }
                        Expression {
                            spelling: None,
                            span: name.span,
                            kind: ExpressionKind::ComponentRef(
                                self.environment.templates[&name.value],
                            ),
                        }
                    };
                    return Ok(ResolvedPart {
                        syntax: part.clone(),
                        kind: PartKind::Dynamic(expression),
                    });
                }
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
                if component.is_none()
                    && name.is_some()
                    && let Some(scope) = attributes
                        .iter()
                        .find(|attribute| attribute.name.value == "slice")
                {
                    let AstExpression::Template {
                        translated: false,
                        segments,
                    } = &scope.value.kind
                    else {
                        return Err(Diagnostic::new(
                            Stage::Resolution,
                            "R011",
                            scope.value.span,
                            "slice requires a static single-quoted name",
                        ));
                    };
                    let mut scope_name = String::new();
                    for segment in segments {
                        let SegmentKind::Text(text) = &segment.kind else {
                            return Err(Diagnostic::new(
                                Stage::Resolution,
                                "R011",
                                segment.span,
                                "slice cannot contain interpolation",
                            ));
                        };
                        scope_name.push_str(text);
                    }
                    if !self.environment.slices.contains_key(&scope_name) {
                        return Err(Diagnostic::new(
                            Stage::Resolution,
                            "R011",
                            scope.value.span,
                            format!("unknown exported slice `{scope_name}`"),
                        ));
                    }
                    self.slice = Some(scope_name);
                }
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
                        && !matches!(attribute.name.value.as_str(), "layout" | "styles" | "focus")
                        && !attribute.name.value.starts_with('@')
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
                    if attribute.name.value == "slice" && component.is_none() {
                        continue;
                    }
                    self.event = if let Some(event_name) = attribute.name.value.strip_prefix('@') {
                        let Some(component) = &component else {
                            return Err(Diagnostic::new(
                                Stage::Resolution,
                                "R012",
                                attribute.name.span,
                                "containers do not declare component events",
                            ));
                        };
                        Some(
                            component
                                .events
                                .iter()
                                .find(|event| event.name == event_name)
                                .ok_or_else(|| {
                                    Diagnostic::new(
                                        Stage::Resolution,
                                        "R012",
                                        attribute.name.span,
                                        format!(
                                            "unknown event `{event_name}`; available: {}",
                                            component
                                                .events
                                                .iter()
                                                .map(|event| event.name)
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        ),
                                    )
                                })?
                                .payload,
                        )
                    } else {
                        None
                    };
                    let value = self.expression(&attribute.value)?;
                    self.event = None;
                    resolved.push((attribute.name.clone(), value));
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
                if binding.value == "props" {
                    return Err(Diagnostic::new(
                        Stage::Resolution,
                        "R017",
                        binding.span,
                        "props is reserved for composite parameters",
                    ));
                }
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

                let (root, skip) = if first.value == "props" {
                    let name = path.get(1).ok_or_else(|| {
                        Diagnostic::new(
                            Stage::Resolution,
                            "R017",
                            first.span,
                            "props requires a declared field",
                        )
                    })?;
                    let (index, prop) = self
                        .environment
                        .props
                        .iter()
                        .enumerate()
                        .find(|(_, prop)| prop.name == name.value)
                        .ok_or_else(|| {
                            Diagnostic::new(
                                Stage::Resolution,
                                "R017",
                                name.span,
                                format!("unknown composite prop `{}`", name.value),
                            )
                        })?;
                    (
                        Root::Prop {
                            index,
                            ty: prop.ty.clone(),
                        },
                        2,
                    )
                } else if let Some(payload) = self.event.filter(|payload| {
                    first.value == "event"
                        || payload
                            .fields()
                            .iter()
                            .any(|field| field.name == first.value)
                }) {
                    (Root::Event(payload), usize::from(first.value == "event"))
                } else if let Some(depth) = self
                    .locals
                    .iter()
                    .rev()
                    .position(|name| name == &first.value)
                {
                    (Root::Local(depth), 1)
                } else if let Some((index, prop)) =
                    self.environment.props.iter().enumerate().find(|(_, prop)| {
                        prop.name == first.value
                            && !matches!(first.value.as_str(), "entities" | "collections" | "event")
                    })
                {
                    (
                        Root::Prop {
                            index,
                            ty: prop.ty.clone(),
                        },
                        1,
                    )
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
                let (slice, action) = match path.as_slice() {
                    [action] => (
                        self.slice.as_ref().ok_or_else(|| {
                            Diagnostic::new(
                                Stage::Resolution,
                                "R007",
                                action.span,
                                "unqualified action requires an enclosing container slice",
                            )
                        })?,
                        action,
                    ),
                    [slice, action] => (&slice.value, action),
                    _ => {
                        return Err(Diagnostic::new(
                            Stage::Resolution,
                            "R007",
                            expression.span,
                            "expected slice.action or action",
                        ));
                    }
                };
                if !self.environment.slices.contains_key(slice) {
                    return Err(Diagnostic::new(
                        Stage::Resolution,
                        "R008",
                        path[0].span,
                        format!("unknown exported slice `{slice}`"),
                    ));
                }
                let handle = *self
                    .environment
                    .actions
                    .get(&(slice.clone(), action.value.clone()))
                    .ok_or_else(|| {
                        Diagnostic::new(
                            Stage::Resolution,
                            "R008",
                            action.span,
                            format!("unknown exported action `{slice}.{}`", action.value),
                        )
                    })?;
                ExpressionKind::Call {
                    handle,
                    arguments: arguments
                        .iter()
                        .map(|argument| {
                            Ok((argument.name.clone(), self.expression(&argument.value)?))
                        })
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
