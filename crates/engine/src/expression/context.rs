use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::DynamicObject;

use crate::application::app::Application;

/// Read-only inputs for expressions: application storage and an optional current value.
/// Results borrow these inputs rather than owning or copying application data.
pub struct ExpressionContext<'a> {
    event: Option<&'a DynamicObject<'a>>,
    language: crate::i18n::indices::LanguageIndex,
    application: Option<&'a Application>,
    locals: Vec<(
        &'a DynamicObject<'a>,
        Option<(crate::application::collection_key::CollectionKey, usize)>,
    )>,
    value: Option<&'a DynamicObject<'a>>,
}

impl<'a> ExpressionContext<'a> {
    /// Application expressions are available; no current field context is set.
    pub fn new(application: &'a Application) -> Self {
        Self {
            event: None,
            language: Default::default(),
            application: Some(application),
            locals: vec![],
            value: None,
        }
    }

    /// Field expressions are available without application storage.
    pub fn from_value(value: &'a DynamicObject<'a>) -> Self {
        Self {
            event: None,
            language: Default::default(),
            application: None,
            locals: vec![],
            value: Some(value),
        }
    }

    /// Selects translation language while preserving application and current value.
    pub fn with_language(mut self, language: crate::i18n::indices::LanguageIndex) -> Self {
        self.language = language;
        self
    }

    pub(crate) fn with_event(mut self, event: &'a DynamicObject<'a>) -> Self {
        self.event = Some(event);
        self
    }
    pub(crate) fn event(&self) -> PixuiResult<&'a DynamicObject<'a>> {
        self.event
            .ok_or_else(|| pixui_error!("event parameters require callback evaluation"))
    }

    pub fn language(&self) -> crate::i18n::indices::LanguageIndex {
        self.language
    }

    pub fn application(&self) -> PixuiResult<&'a Application> {
        self.application
            .ok_or_else(|| pixui_error!("expression requires application context"))
    }

    pub fn value(&self) -> PixuiResult<&'a DynamicObject<'a>> {
        self.value
            .ok_or_else(|| pixui_error!("expression requires a current value"))
    }

    /// Indexed lexical loop inputs; zero selects the innermost loop.
    pub fn local(&self, depth: usize) -> PixuiResult<&'a DynamicObject<'a>> {
        self.locals
            .iter()
            .rev()
            .nth(depth)
            .map(|(value, _)| *value)
            .ok_or_else(|| pixui_error!("invalid loop scope depth"))
    }
    /// Address of a collection-backed loop item. Positions are used only during
    /// the immutable render to immediately mint a checked generational handle.
    pub fn local_address(
        &self,
        depth: usize,
    ) -> PixuiResult<(crate::application::collection_key::CollectionKey, usize)> {
        self.locals
            .iter()
            .rev()
            .nth(depth)
            .and_then(|(_, address)| *address)
            .ok_or_else(|| pixui_error!("action item references require a collection-backed loop"))
    }
    /// Sets the current loop element and retains access to enclosing loops.
    pub fn with_value<'b>(&self, value: &'b DynamicObject<'b>) -> ExpressionContext<'b>
    where
        'a: 'b,
    {
        self.with_loop_value(value, None)
    }
    pub(crate) fn with_loop_value<'b>(
        &self,
        value: &'b DynamicObject<'b>,
        address: Option<(crate::application::collection_key::CollectionKey, usize)>,
    ) -> ExpressionContext<'b>
    where
        'a: 'b,
    {
        let mut locals = self.locals.clone();
        locals.push((value, address));
        ExpressionContext {
            event: self.event,
            language: self.language,
            application: self.application,
            value: Some(value),
            locals,
        }
    }
}
