//! Owned full-content proposals dispatched through ordinary application actions.
use super::super::presentation::PresentationSettings;
use crate::{
    application::{action::ActionCall, app::Application},
    expression::context::ExpressionContext,
};
use pixui_base::PixuiResult;

/// Called once per changed-content proposal. Build an action with the full new
/// value; content remains controlled by the application's subsequently resolved props.
pub type ChangeBinding = Box<dyn Fn(&Application, String) -> PixuiResult<ActionCall> + Send>;
pub type ChangeFactory =
    for<'a> fn(&ExpressionContext<'a>, &PresentationSettings) -> PixuiResult<ChangeBinding>;
