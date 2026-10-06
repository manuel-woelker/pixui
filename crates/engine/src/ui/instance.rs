//! Worker-side UI state for one native window or headless rendering target.

use super::{
    activation::ActionBinding, definition::UiDefinitionId, display_list::RenderRevision,
    geometry::Rect, mailbox::OutputSender, presentation::PresentationSettings,
};
use crate::live_model::state::LiveState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiInstanceId(pub(crate) u64);

/// Clipped logical bounds and a worker-local activation binding.
pub struct HitRegion {
    pub bounds: Rect,
    /// Component position in the prepared tree, including noninteractive nodes.
    pub component_index: usize,
    pub(crate) activate: ActionBinding,
}

/// Geometry from the last successful render. Component bounds follow physical
/// preorder, while hit regions contain only interactive widgets in paint order.
#[derive(Default)]
pub struct LayoutState {
    pub component_bounds: Vec<Rect>,
    pub hit_regions: Vec<HitRegion>,
    pub content_height: f32,
    /// Derived geometry: shared requested scroll clamped for this viewport.
    pub scroll_offset: f32,
}

/// All geometry and interaction live on the application worker. Layout belongs
/// to `revision`; visual redraws preserve compatible presented input revisions.
/// Shared focus, hover and requested scrolling belong to the definition.
/// Stale discrete input is rejected and old motion is discarded.
pub struct UiInstance {
    pub(crate) definition: UiDefinitionId,
    pub(crate) state: LiveState,
    pub(crate) settings: PresentationSettings,
    pub(crate) layout: LayoutState,
    pub(crate) revision: RenderRevision,
    pub(crate) compatible_revision: RenderRevision,
    pub(crate) redraw_only: bool,
    pub(crate) dirty: bool,
    pub(crate) geometry_stale: bool,
    pub(crate) outputs: OutputSender,
    pub(crate) error: Option<String>,
    pub(crate) animation_request: Option<u64>,
    pub(crate) visible: bool,
}

impl UiInstance {
    /// Whether worker rendering is enabled. Headless instances start visible.
    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn state(&self) -> &LiveState {
        &self.state
    }
    pub fn settings(&self) -> &PresentationSettings {
        &self.settings
    }
    pub fn layout(&self) -> &LayoutState {
        &self.layout
    }
    pub fn revision(&self) -> RenderRevision {
        self.revision
    }
    pub fn last_error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    /// Effective scroll for this viewport, derived from shared definition state.
    pub fn scroll_offset(&self) -> f32 {
        self.layout.scroll_offset
    }
}
