//! Declarative source messages. Argument expressions are evaluated once per occurrence.
use super::{catalog::MessageKey, indices::MessageIndex, template::CompiledTemplate};
use crate::expression::expression::Expression;
use pixui_base::{PixuiResult, pixui_error};
use std::{collections::BTreeSet, panic::Location};

#[derive(Clone)]
pub struct I18nExpression {
    pub(crate) key: MessageKey,
    pub(crate) arguments: Vec<(String, Expression)>,
    pub(crate) comment: Option<String>,
    pub(crate) location: String,
    pub(crate) index: MessageIndex,
}
impl I18nExpression {
    #[track_caller]
    pub fn new(
        source: impl Into<String>,
        arguments: impl IntoIterator<Item = (impl Into<String>, Expression)>,
    ) -> PixuiResult<Self> {
        let location = Location::caller();
        let value = Self {
            key: MessageKey {
                source: source.into(),
                context: None,
            },
            arguments: arguments
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
            comment: None,
            location: format!("{}:{}", location.file(), location.line()),
            index: MessageIndex::default(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.key.context = Some(context.into());
        self
    }
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }
    pub fn index(&self) -> MessageIndex {
        self.index
    }
    pub fn source(&self) -> &str {
        &self.key.source
    }
    pub(crate) fn validate(&self) -> PixuiResult<CompiledTemplate> {
        if self.key.source.is_empty() {
            return Err(pixui_error!("i18n source must not be empty"));
        }
        let template = CompiledTemplate::parse(&self.key.source)?;
        let names: BTreeSet<_> = self
            .arguments
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        if names.len() != self.arguments.len() || &names != template.names() {
            return Err(pixui_error!(
                "i18n arguments must match source placeholders exactly"
            ));
        }
        Ok(template)
    }
}
