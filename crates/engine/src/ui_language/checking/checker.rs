//! Type checking does not run props resolvers or mutate application state.
use super::{
    conversions::value_type,
    typed::{ExpressionKind, PartKind, TypedDefinition, TypedPart},
};
use crate::{
    expression::expression::ExpressionKind as RuntimeKind,
    layout::container::ContainerPart,
    ui_language::{
        ast::part::PartKind as AstPart,
        diagnostic::report::{Diagnostic, Stage},
        resolution::{
            environment::Environment,
            resolved::{self, ResolvedDefinition, ResolvedPart, Root, Segment},
        },
        source::span::Span,
    },
};
use pixui_reflect::TypeDescriptor;
use std::{any::TypeId, collections::HashMap};
pub(super) type CheckResult<T> = Result<T, Diagnostic>;
pub fn check(
    definition: ResolvedDefinition,
    environment: &Environment,
) -> Result<TypedDefinition, Vec<Diagnostic>> {
    let mut checker = Checker {
        environment,
        locals: vec![],
        collection_locals: vec![],
        errors: vec![],
        callback: false,
    };
    match checker.part(definition.root) {
        Ok(root) if checker.errors.is_empty() => Ok(TypedDefinition {
            syntax: definition.syntax,
            root,
        }),
        Ok(_) => Err(checker.errors),
        Err(error) => {
            if checker.errors.len() < crate::ui_language::lexer::scan::MAX_DIAGNOSTICS {
                checker.errors.push(error);
            }
            Err(checker.errors)
        }
    }
}
pub(super) struct Checker<'a> {
    pub(super) environment: &'a Environment,
    pub(super) locals: Vec<&'static TypeDescriptor>,
    pub(super) collection_locals: Vec<bool>,
    pub(super) callback: bool,
    pub(super) errors: Vec<Diagnostic>,
}
impl Checker<'_> {
    fn children(&mut self, children: Vec<ResolvedPart>) -> Vec<TypedPart> {
        let mut result = vec![];
        for child in children {
            match self.part(child) {
                Ok(child) => result.push(child),
                Err(error)
                    if self.errors.len() < crate::ui_language::lexer::scan::MAX_DIAGNOSTICS =>
                {
                    self.errors.push(error)
                }
                Err(_) => break,
            }
        }
        result
    }
    pub(super) fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Stage::Checking, "T004", span, message)
    }
    fn part(&mut self, part: ResolvedPart) -> CheckResult<TypedPart> {
        let span = part.syntax.span;
        let kind = match part.kind {
            resolved::PartKind::Dynamic(expression) => PartKind::Dynamic(self.expression(
                &expression,
                Some(TypeId::of::<crate::composition::component_ref::ComponentRef>()),
            )?),
            resolved::PartKind::Element {
                component: Some(export),
                attributes,
                children,
            } => {
                if !children.is_empty() {
                    return Err(self.error(span, "leaf components cannot have children"));
                }
                let mut values: HashMap<_, _> = attributes
                    .into_iter()
                    .map(|(name, expression)| (name.value, expression))
                    .collect();
                let event_names = values
                    .keys()
                    .filter(|name| name.starts_with('@'))
                    .cloned()
                    .collect::<Vec<_>>();
                let mut callbacks = vec![];
                for name in event_names {
                    let expression = values.remove(&name).expect("present callback");
                    self.callback = true;
                    let action = self.action(&expression);
                    self.callback = false;
                    callbacks.push((name[1..].to_owned(), action?));
                }
                let layout = values
                    .remove("layout")
                    .map(|expression| self.layout_export(&expression))
                    .transpose()?
                    .unwrap_or_default();
                let styles = values
                    .remove("styles")
                    .map(|expression| self.style_export(&expression))
                    .transpose()?
                    .unwrap_or_default();
                let focus = values
                    .remove("focus")
                    .map(|expression| {
                        use crate::ui::focus::FocusBehavior;
                        match self.literal_text(&expression)?.as_str() {
                            "automatic" => Ok(FocusBehavior::Automatic),
                            "sequential" => Ok(FocusBehavior::Sequential),
                            "direct" => Ok(FocusBehavior::Direct),
                            "none" => Ok(FocusBehavior::None),
                            _ => Err(self.error(
                                expression.span,
                                "focus must be automatic, sequential, direct or none",
                            )),
                        }
                    })
                    .transpose()?
                    .unwrap_or_default();
                let mut props = vec![];
                let mut types = vec![];
                for field in export.props.fields() {
                    let expression = values.remove(field.name).ok_or_else(|| {
                        self.error(span, format!("missing required property `{}`", field.name))
                    })?;
                    let id = field.type_id().ok_or_else(|| {
                        self.error(expression.span, "property has no type metadata")
                    })?;
                    props.push(self.expression(&expression, Some(id))?);
                    types.push(value_type(self.environment, id, expression.span)?);
                }
                PartKind::Component {
                    export,
                    props,
                    types,
                    callbacks,
                    layout,
                    styles,
                    focus,
                }
            }
            resolved::PartKind::Element {
                component: None,
                attributes,
                children,
            } => {
                let AstPart::Element { name, .. } = part.syntax.kind else {
                    unreachable!()
                };
                let Some(name) = name else {
                    if !attributes.is_empty() {
                        return Err(self.error(span, "fragments cannot have attributes"));
                    }
                    return Ok(TypedPart {
                        span,
                        kind: PartKind::Composite(self.children(children)),
                    });
                };
                let mut container = match name.value.as_str() {
                    "Row" => ContainerPart::row(),
                    "Column" => ContainerPart::column(),
                    "Grid" => ContainerPart::grid(),
                    _ => unreachable!(),
                };
                // Apply the exported base first; explicit attributes always override it.
                if let Some((_, expression)) =
                    attributes.iter().find(|(name, _)| name.value == "layout")
                {
                    container.style = self.layout_export(expression)?;
                }
                // Uniform insets provide the base for edge-specific overrides.
                for (name, expression) in attributes
                    .iter()
                    .filter(|(name, _)| matches!(name.value.as_str(), "padding" | "margin"))
                {
                    self.container_attribute(&mut container, name, expression)?;
                }
                for (name, expression) in attributes {
                    if !matches!(name.value.as_str(), "padding" | "margin") {
                        self.container_attribute(&mut container, &name, &expression)?;
                    }
                }
                container
                    .to_taffy()
                    .map_err(|error| self.error(span, error.to_string()))?;
                PartKind::Container {
                    container,
                    children: self.children(children),
                }
            }
            resolved::PartKind::For { sequence, body } => {
                let sequence = self.expression(&sequence, None)?;
                let element = sequence.descriptor.element_type().ok_or_else(|| {
                    self.error(sequence.span, "for expression must be a sequence")
                })?;
                let collection_backed = matches!(&sequence.kind,ExpressionKind::Path {root:Root::Data(data),fields,..} if fields.is_empty() && matches!(data.expression.kind(),RuntimeKind::Collection(_)));
                self.collection_locals.push(collection_backed);
                self.locals.push(element);
                let body = self.part(*body);
                self.locals.pop();
                self.collection_locals.pop();
                PartKind::For {
                    sequence,
                    body: Box::new(body?),
                }
            }
            resolved::PartKind::Match { value, arms } => {
                let value = self.expression(&value, None)?;
                let id = value.descriptor.type_id();
                let mut patterns = HashMap::new();
                let mut wildcard = false;
                let mut checked = vec![];
                let mut booleans = [false, false];
                for (pattern, body, arm_span) in arms {
                    if wildcard {
                        return Err(self.error(arm_span, "unreachable arm after wildcard"));
                    }
                    let pattern = if let Some(pattern) = pattern {
                        let typed = self.expression(&pattern, Some(id))?;
                        let key = match &typed.kind {
                            ExpressionKind::Constant { value, .. }
                                if id == TypeId::of::<bool>() =>
                            {
                                let value = *value.downcast_ref::<bool>().unwrap();
                                booleans[usize::from(value)] = true;
                                value.to_string()
                            }
                            ExpressionKind::Constant { value, .. } => format!(
                                "{:?}",
                                crate::expression::program::scalar_snapshot(value.as_ref(), id)
                                    .map_err(|error| self.error(pattern.span, error.to_string()))?
                            ),
                            ExpressionKind::Template {
                                translated: false,
                                source,
                                arguments,
                            } if arguments.is_empty() => source.clone(),
                            _ => {
                                return Err(self.error(
                                    pattern.span,
                                    "match patterns must be untranslated scalar literals",
                                ));
                            }
                        };
                        if let Some(previous) = patterns.insert(key, pattern.span) {
                            return Err(self
                                .error(pattern.span, "duplicate match pattern")
                                .secondary(previous, "first pattern"));
                        }
                        Some(typed)
                    } else {
                        wildcard = true;
                        None
                    };
                    checked.push((pattern, self.part(body)?));
                }
                if !wildcard && !(id == TypeId::of::<bool>() && booleans == [true, true]) {
                    return Err(
                        self.error(span, "match requires a wildcard or exhaustive boolean arms")
                    );
                }
                PartKind::Match {
                    value,
                    arms: checked,
                }
            }
        };
        Ok(TypedPart { span, kind })
    }
    pub(super) fn literal_text(&self, expression: &resolved::Expression) -> CheckResult<String> {
        if let resolved::ExpressionKind::Template {
            translated: false,
            segments,
        } = &expression.kind
        {
            let mut text = String::new();
            for segment in segments {
                match segment {
                    Segment::Text(value, _) => text.push_str(value),
                    _ => {
                        return Err(self.error(
                            expression.span,
                            "expected an untranslated literal without interpolation",
                        ));
                    }
                }
            }
            Ok(text)
        } else {
            Err(self.error(
                expression.span,
                "expected an untranslated single-quoted literal",
            ))
        }
    }
    fn layout_export(
        &self,
        expression: &resolved::Expression,
    ) -> CheckResult<crate::layout::style::LayoutStyle> {
        if let resolved::ExpressionKind::Path {
            root: Root::Layout(layout),
            fields,
        } = &expression.kind
            && fields.is_empty()
        {
            layout
                .to_taffy()
                .map_err(|error| self.error(expression.span, error.to_string()))?;
            Ok((**layout).clone())
        } else {
            Err(self.error(
                expression.span,
                "layout requires an exported layouts.name value",
            ))
        }
    }
    fn style_export(
        &self,
        expression: &resolved::Expression,
    ) -> CheckResult<Vec<crate::styles::slots::SlotAssignment>> {
        if let resolved::ExpressionKind::Path {
            root: Root::Styles(styles),
            fields,
        } = &expression.kind
            && fields.is_empty()
        {
            Ok(styles.clone())
        } else {
            Err(self.error(
                expression.span,
                "styles requires an exported styles.name value",
            ))
        }
    }
}
