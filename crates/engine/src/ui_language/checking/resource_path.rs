//! Contextual conversion to validated resource filenames; never translates paths.
use super::{
    checker::{CheckResult, Checker},
    typed::{Expression, ExpressionKind},
};
use crate::{
    resources::path::ResourcePath,
    ui_language::resolution::{
        environment::ValueType,
        resolved::{self, Segment},
    },
};
use pixui_reflect::Reflect;
use std::sync::Arc;
impl Checker<'_> {
    pub(super) fn resource_path(&self, value: Expression) -> CheckResult<Expression> {
        let span = value.span;
        if contains_localized(&value) {
            return Err(self.error(span, "resource paths cannot contain localized text"));
        }
        let kind = if let ExpressionKind::Template {
            translated: false,
            source,
            arguments,
        } = &value.kind
            && arguments.is_empty()
        {
            let text = crate::i18n::template::CompiledTemplate::parse(source)
                .and_then(|template| template.render(&[]))
                .map_err(|error| self.error(span, error.to_string()))?;
            let path =
                ResourcePath::new(text).map_err(|error| self.error(span, error.to_string()))?;
            ExpressionKind::Constant {
                value: Arc::new(path),
                ty: ValueType::of::<ResourcePath>(),
            }
        } else {
            ExpressionKind::ResourcePath(Box::new(value))
        };
        Ok(Expression {
            kind,
            descriptor: ResourcePath::type_descriptor(),
            span,
        })
    }
    pub(super) fn untranslated_path(&self, expression: &resolved::Expression) -> CheckResult<()> {
        match &expression.kind {
            resolved::ExpressionKind::Template {
                translated: true, ..
            } => {
                return Err(self.error(
                    expression.span,
                    "resource paths cannot be translated; use single-quoted templates",
                ));
            }
            resolved::ExpressionKind::Template { segments, .. } => {
                for segment in segments {
                    if let Segment::Value(value) = segment {
                        self.untranslated_path(value)?;
                    }
                }
            }
            resolved::ExpressionKind::Not(value) => self.untranslated_path(value)?,
            resolved::ExpressionKind::Binary { left, right, .. } => {
                self.untranslated_path(left)?;
                self.untranslated_path(right)?;
            }
            resolved::ExpressionKind::Call { arguments, .. } => {
                for (_, value) in arguments {
                    self.untranslated_path(value)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Known localized handles cannot be smuggled into structural path templates.
fn contains_localized(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::LocalizedText(_) => true,
        ExpressionKind::Template { arguments, .. } => {
            arguments.iter().any(|(_, value)| contains_localized(value))
        }
        ExpressionKind::ResourcePath(value) | ExpressionKind::Not(value) => {
            contains_localized(value)
        }
        ExpressionKind::Binary { left, right, .. } => {
            contains_localized(left) || contains_localized(right)
        }
        _ => false,
    }
}
