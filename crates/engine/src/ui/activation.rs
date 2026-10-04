//! Behavior bindings independent of component appearance.

use super::presentation::PresentationSettings;
use crate::{
    application::{action::ActionCall, app::Application},
    expression::context::ExpressionContext,
};
use pixui_base::PixuiResult;

/// Builds a fresh action call for every activation. Capture opaque references
/// and cached handles; dispatch checks identities and generations again.
pub type ActionBinding = Box<dyn Fn(&Application) -> PixuiResult<ActionCall> + Send>;

/// Resolves the activation once per physical component per render, independently
/// of its typed props and painter. No application borrows may escape.
pub type ActivationFactory =
    for<'a> fn(&ExpressionContext<'a>, &PresentationSettings) -> PixuiResult<ActionBinding>;
