use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::DynamicObject;

use crate::application::app::Application;

/// Read-only inputs for expressions: application storage and an optional current value.
/// Results borrow these inputs rather than owning or copying application data.
pub struct ExpressionContext<'a> {
    application: Option<&'a Application>,
    value: Option<&'a DynamicObject<'a>>,
}

impl<'a> ExpressionContext<'a> {
    /// Application expressions are available; no current field context is set.
    pub fn new(application: &'a Application) -> Self {
        Self {
            application: Some(application),
            value: None,
        }
    }

    /// Field expressions are available without application storage.
    pub fn from_value(value: &'a DynamicObject<'a>) -> Self {
        Self {
            application: None,
            value: Some(value),
        }
    }

    pub fn application(&self) -> PixuiResult<&'a Application> {
        self.application
            .ok_or_else(|| pixui_error!("expression requires application context"))
    }

    pub fn value(&self) -> PixuiResult<&'a DynamicObject<'a>> {
        self.value
            .ok_or_else(|| pixui_error!("expression requires a current value"))
    }

    /// Sets the current loop element while retaining application access.
    pub fn with_value<'b>(&self, value: &'b DynamicObject<'b>) -> ExpressionContext<'b>
    where
        'a: 'b,
    {
        ExpressionContext {
            application: self.application,
            value: Some(value),
        }
    }
}
