#![doc = include_str!("Window properties.md")]

//! Worker-resolved native window metadata, independent of painting.
use super::{image::Image, presentation::PresentationSettings};
use crate::{expression::context::ExpressionContext, resources::path::ResourcePath};
use pixui_base::{PixuiResult, PixuiString};

/// Desired properties for one window. `None` explicitly clears its icon.
/// Paths use the application's configured image service, including optional
/// background hot reload and its session-owned snapshots.
pub struct WindowProperties {
    pub title: PixuiString,
    pub icon: Option<ResourcePath>,
}

/// Runs on the worker with application data and this instance's presentation.
/// Errors retain the last published properties; no partially resolved update is
/// sent. Without hot reload, icon loading may block this worker. Watched icons
/// resolve background-prepared snapshots, independently of component painting.
/// Resolved after content/presentation changes and explicit redraws, including
/// hidden windows. Animation-only and diagnostic refreshes do not reevaluate it.
pub type WindowPropertiesResolver =
    for<'a> fn(&ExpressionContext<'a>, &PresentationSettings) -> PixuiResult<WindowProperties>;

/// Latest-value property commands, not ordered events such as request-focus.
/// The mailbox coalesces each property independently, preserving the last title
/// AND icon even when the GUI has not drained earlier commands.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowCommand {
    SetTitle(PixuiString),
    SetIcon(Option<Image>),
}

#[derive(PartialEq)]
pub(crate) struct ResolvedWindowProperties {
    pub title: PixuiString,
    pub icon: Option<Image>,
}

/// Explicit expression-backed native metadata. Expressions are registered with
/// the enclosing definition's domain and evaluated in declaration order.
pub type ExpressionWindowPropertiesResolver = for<'a> fn(
    &ExpressionContext<'a>,
    &PresentationSettings,
    &[pixui_reflect::DynamicObject<'a>],
) -> PixuiResult<WindowProperties>;

pub(crate) struct WindowPropertyExpressions {
    pub expressions: Vec<crate::expression::expression::Expression>,
    pub resolve: ExpressionWindowPropertiesResolver,
}
