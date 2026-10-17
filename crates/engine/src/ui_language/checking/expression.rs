//! Expression and deferred-action type rules.
use super::{
    checker::{CheckResult, Checker},
    conversions::{number, value_type},
    typed::{Action, ActionArgument, Expression, ExpressionKind, ReferenceRoot},
};
use crate::{
    expression::expression::ExpressionKind as RuntimeKind,
    ui_language::{
        ast::expression::Operator,
        diagnostic::report::{Diagnostic, Stage},
        resolution::{
            environment::ValueType,
            resolved::{self, Root, Segment},
        },
    },
};
use pixui_reflect::Reflect;
use std::{any::TypeId, collections::HashMap, sync::Arc};
impl Checker<'_> {
    pub(super) fn expression(
        &self,
        expression: &resolved::Expression,
        expected: Option<TypeId>,
    ) -> CheckResult<Expression> {
        let span = expression.span;
        let resource_path = expected == Some(TypeId::of::<crate::resources::path::ResourcePath>());
        if resource_path {
            self.untranslated_path(expression)?;
        }
        let (kind, descriptor) = match &expression.kind {
            resolved::ExpressionKind::ComponentRef(reference) => (
                ExpressionKind::Constant {
                    value: Arc::new(*reference),
                    ty: ValueType::of::<crate::composition::component_ref::ComponentRef>(),
                },
                crate::composition::component_ref::ComponentRef::type_descriptor(),
            ),
            resolved::ExpressionKind::Bool(value) => (
                ExpressionKind::Constant {
                    value: Arc::new(*value),
                    ty: ValueType::of::<bool>(),
                },
                bool::type_descriptor(),
            ),
            resolved::ExpressionKind::Number(text) => return number(text, expected, span),
            resolved::ExpressionKind::Path { root, fields } => {
                let mut descriptor = match root {
                    Root::Data(data) => data.descriptor,
                    Root::Event(payload) => payload,
                    Root::Local(depth) => self.locals[self.locals.len() - 1 - depth],
                    Root::Layout(_) | Root::Styles(_) => {
                        return Err(self.error(
                            span,
                            "layout/styles exports only allowed on their reserved attributes",
                        ));
                    }
                };
                let capture_type = if self.callback && !matches!(root, Root::Event(_)) {
                    Some(value_type(self.environment, descriptor.type_id(), span)?)
                } else {
                    None
                };
                let mut indices = vec![];
                for name in fields {
                    let index = descriptor.field_index(&name.value).map_err(|_| {
                        Diagnostic::new(
                            Stage::Resolution,
                            "R009",
                            name.span,
                            format!("unknown field `{}`", name.value),
                        )
                    })?;
                    let id = descriptor.fields()[index.0]
                        .type_id()
                        .ok_or_else(|| self.error(name.span, "field has no type metadata"))?;
                    descriptor = *self.environment.descriptors.get(&id).ok_or_else(|| {
                        self.error(name.span, "field type needs exported reflection metadata")
                    })?;
                    indices.push((index, descriptor));
                }
                (
                    ExpressionKind::Path {
                        root: root.clone(),
                        capture_result: if capture_type.is_some() {
                            Some(value_type(self.environment, descriptor.type_id(), span)?)
                        } else {
                            None
                        },
                        capture_type,
                        fields: indices,
                    },
                    descriptor,
                )
            }
            resolved::ExpressionKind::Template {
                translated,
                segments,
            } => {
                let mut source = String::new();
                let mut arguments = vec![];
                let mut names: HashMap<String, String> = HashMap::new();
                for (position, segment) in segments.iter().enumerate() {
                    match segment {
                        Segment::Text(text, _) => {
                            source.push_str(&text.replace('{', "{{").replace('}', "}}"));
                        }
                        Segment::Value(value) => {
                            let typed = self.expression(value, None)?;
                            let typed = if typed.descriptor.type_id()
                                == TypeId::of::<crate::composition::localized_text::LocalizedText>()
                            {
                                Expression {
                                    span: typed.span,
                                    descriptor: String::type_descriptor(),
                                    kind: ExpressionKind::LocalizedText(Box::new(typed)),
                                }
                            } else {
                                typed
                            };
                            if !crate::expression::program::is_scalar(typed.descriptor.type_id()) {
                                return Err(
                                    self.error(value.span, "template arguments must be scalars")
                                );
                            }
                            let (name, identity) = if let Some(spelling) = &value.spelling {
                                (spelling.replace('.', "_"), spelling.clone())
                            } else {
                                (
                                    format!("arg{position}"),
                                    format!("expression at {}", value.span.start),
                                )
                            };
                            if let Some(previous) = names.get(&name) {
                                if previous != &identity {
                                    return Err(self.error(
                                        value.span,
                                        "interpolation placeholder name collision",
                                    ));
                                }
                            } else {
                                names.insert(name.clone(), identity);
                                arguments.push((name.clone(), typed));
                            }
                            source.push('{');
                            source.push_str(&name);
                            source.push('}');
                        }
                    }
                }
                crate::i18n::template::CompiledTemplate::parse(&source)
                    .map_err(|error| self.error(span, error.to_string()))?;
                (
                    ExpressionKind::Template {
                        translated: *translated,
                        source,
                        arguments,
                    },
                    String::type_descriptor(),
                )
            }
            resolved::ExpressionKind::Not(value) => (
                ExpressionKind::Not(Box::new(
                    self.expression(value, Some(TypeId::of::<bool>()))?,
                )),
                bool::type_descriptor(),
            ),
            resolved::ExpressionKind::Binary { op, left, right } => {
                let expect =
                    matches!(op, Operator::And | Operator::Or).then_some(TypeId::of::<bool>());
                // Infer equality literal type from its nonliteral partner.
                let (left, right) = if matches!(left.kind, resolved::ExpressionKind::Number(_))
                    && !matches!(right.kind, resolved::ExpressionKind::Number(_))
                {
                    let right = self.expression(right, expect)?;
                    let left = self.expression(left, Some(right.descriptor.type_id()))?;
                    (left, right)
                } else {
                    let left = self.expression(left, expect)?;
                    let right = self.expression(right, Some(left.descriptor.type_id()))?;
                    (left, right)
                };
                if !crate::expression::program::is_scalar(left.descriptor.type_id()) {
                    return Err(self.error(span, "comparison requires scalar values"));
                }
                (
                    ExpressionKind::Binary {
                        op: *op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    bool::type_descriptor(),
                )
            }
            resolved::ExpressionKind::Call { .. } => {
                return Err(self.error(span, "action calls are only allowed in event attributes"));
            }
        };
        if expected.is_some()
            && matches!(&kind,ExpressionKind::Path {root:Root::Data(data),fields,..} if fields.is_empty() && matches!(data.expression.kind(),RuntimeKind::Collection(_)))
        {
            return Err(self.error(
                span,
                "collections are borrowed sequences, not owned props or action values",
            ));
        }
        if resource_path && descriptor.type_id() == TypeId::of::<String>() {
            return self.resource_path(Expression {
                kind,
                descriptor,
                span,
            });
        }
        if expected == Some(TypeId::of::<String>())
            && descriptor.type_id()
                == TypeId::of::<crate::composition::localized_text::LocalizedText>()
        {
            return Ok(Expression {
                kind: ExpressionKind::LocalizedText(Box::new(Expression {
                    kind,
                    descriptor,
                    span,
                })),
                descriptor: String::type_descriptor(),
                span,
            });
        }
        if expected.is_some_and(|expected| expected != descriptor.type_id()) {
            let mut error = self.error(
                span,
                format!("incompatible expression type `{}`", descriptor.type_name()),
            );
            if matches!(
                &kind,
                ExpressionKind::Path {
                    root: Root::Event(_),
                    ..
                }
            ) {
                error.notes.push("Event parameters shadow enclosing loop locals inside callbacks; rename the loop variable if you need both values.".into());
            }
            return Err(error);
        }
        Ok(Expression {
            kind,
            descriptor,
            span,
        })
    }
    pub(super) fn action(&self, expression: &resolved::Expression) -> CheckResult<Action> {
        let resolved::ExpressionKind::Call { handle, arguments } = &expression.kind else {
            return Err(self.error(expression.span, "event requires an exported action call"));
        };
        let fields = handle.descriptor().arguments().fields();
        let mut named = HashMap::new();
        for (name, value) in arguments {
            if !fields.iter().any(|field| field.name == name.value) {
                return Err(self.error(
                    name.span,
                    format!(
                        "unknown action argument `{}`; available: {}",
                        name.value,
                        fields
                            .iter()
                            .map(|field| field.name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
            if let Some((previous, _)) = named.insert(name.value.as_str(), (name.span, value)) {
                return Err(self
                    .error(name.span, "duplicate action argument")
                    .secondary(previous, "first argument"));
            }
        }
        let mut checked = vec![];
        for field in fields {
            let (_, argument) = named.remove(field.name).ok_or_else(|| {
                self.error(
                    expression.span,
                    format!("missing action argument `{}`", field.name),
                )
            })?;
            let id = field
                .type_id()
                .ok_or_else(|| self.error(argument.span, "action field lacks type metadata"))?;
            if let Some(reference) = self.environment.references.get(&id) {
                let root =
                    match &argument.kind {
                        resolved::ExpressionKind::Path {
                            root: Root::Local(depth),
                            fields,
                        } if fields.is_empty()
                            && self.collection_locals[self.collection_locals.len() - 1 - depth]
                            && self.locals[self.locals.len() - 1 - depth].type_id()
                                == reference.item =>
                        {
                            ReferenceRoot::Local(*depth)
                        }
                        resolved::ExpressionKind::Path {
                            root: Root::Data(data),
                            fields,
                        } if fields.is_empty() && data.descriptor.type_id() == reference.item => {
                            match data.expression.kind() {
                                RuntimeKind::Entity(reference) => {
                                    ReferenceRoot::Entity(reference.clone())
                                }
                                _ => return Err(self.error(
                                    argument.span,
                                    "action reference requires an entity or collection loop item",
                                )),
                            }
                        }
                        _ => {
                            return Err(self.error(
                                argument.span,
                                "action reference requires an entity or collection loop item",
                            ));
                        }
                    };
                checked.push(ActionArgument::Reference(root, reference.clone()));
            } else {
                checked.push(ActionArgument::Value(
                    self.expression(argument, Some(id))?,
                    value_type(self.environment, id, argument.span)?,
                ));
            }
        }
        Ok(Action {
            handle: *handle,
            arguments: checked,
            span: expression.span,
        })
    }
}
