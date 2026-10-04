//! Basic component presentation and worker-local event bindings.

use super::presentation::PresentationSettings;
use crate::{
    application::{action::ActionCall, app::Application},
    expression::context::ExpressionContext,
};
use pixui_base::PixuiResult;

/// Builds a fresh owned action call for every activation, revalidating addresses
/// at dispatch. Capture opaque item references, not positional loop indexes.
pub type ActionBinding = Box<dyn Fn(&Application) -> PixuiResult<ActionCall> + Send>;

pub enum Widget {
    Label {
        text: String,
    },
    Button {
        text: String,
        activate: ActionBinding,
    },
    Checkbox {
        text: String,
        checked: bool,
        activate: ActionBinding,
    },
}

/// Resolves a component's current presentation on every walk. Callbacks are
/// read-only with respect to application data and receive instance settings.
pub type WidgetFactory =
    for<'a> fn(&ExpressionContext<'a>, &PresentationSettings) -> PixuiResult<Widget>;
