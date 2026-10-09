//! Worker-side UI state for one native window or headless rendering target.

use super::{
    activation::ActionBinding, definition::UiDefinitionId, display_list::RenderRevision,
    geometry::Rect, mailbox::OutputSender, presentation::PresentationSettings,
};
use crate::live_model::{identity::ComponentPath, state::LiveState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiInstanceId(pub(crate) u64);

/// A clipped pointer region referencing the published component arrays.
pub struct HitRegion {
    pub bounds: Rect,
    /// Component position in the prepared tree, including noninteractive nodes.
    pub component_index: usize,
}

/// Eligible focus targets in physical preorder. Includes offscreen components,
/// but excludes fully clipped descendants of non-scrolling containers.
pub struct FocusTarget {
    pub component_index: usize,
    pub path: ComponentPath,
    pub sequential: bool,
    /// Ancestor-clipped geometry without the viewport's vertical clip.
    pub bounds: Rect,
}

/// Geometry from the last successful render. Component bounds follow physical
/// preorder, while hit regions contain only interactive widgets in paint order.
#[derive(Default)]
pub struct LayoutState {
    pub component_bounds: Vec<Rect>,
    /// Structural identity parallel to the flattened component geometry.
    pub component_paths: Vec<ComponentPath>,
    /// Index lookup in this successful preparation, never a persistent identity.
    pub component_indices: std::collections::HashMap<ComponentPath, usize>,
    pub(crate) activations: Vec<Option<ActionBinding>>,
    /// Focusable pointer regions, independent of activation hit regions.
    pub focus_regions: Vec<HitRegion>,
    pub hit_regions: Vec<HitRegion>,
    pub focus_targets: Vec<FocusTarget>,
    pub content_bounds: Vec<Rect>,
    pub component_clips: Vec<Rect>,
    pub container_bounds: Vec<Rect>,
    pub(crate) component_addresses: Vec<crate::component_registry::component_id::ComponentAddress>,
    pub(crate) activation_factories: Vec<Option<usize>>,
    pub content_height: f32,
    /// Derived geometry: shared requested scroll clamped for this viewport.
    pub scroll_offset: f32,
}

impl LayoutState {
    /// Geometry and registration compatibility for retaining the already
    /// published action snapshot. Content/presentation invalidation separately
    /// forbids reuse; closures cannot be compared for captured-value equality.
    pub(crate) fn compatible_with(&self, previous: &Self) -> bool {
        self.component_paths == previous.component_paths
            && self.component_bounds == previous.component_bounds
            && self.content_bounds == previous.content_bounds
            && self.component_clips == previous.component_clips
            && self.component_addresses == previous.component_addresses
            && self.activation_factories == previous.activation_factories
            && self
                .focus_targets
                .iter()
                .map(|target| (&target.path, target.sequential))
                .eq(previous
                    .focus_targets
                    .iter()
                    .map(|target| (&target.path, target.sequential)))
            && self
                .focus_regions
                .iter()
                .map(|region| (region.bounds, region.component_index))
                .eq(previous
                    .focus_regions
                    .iter()
                    .map(|region| (region.bounds, region.component_index)))
            && self
                .hit_regions
                .iter()
                .map(|region| (region.bounds, region.component_index))
                .eq(previous
                    .hit_regions
                    .iter()
                    .map(|region| (region.bounds, region.component_index)))
    }
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
    pub(crate) window_properties_dirty: bool,
    pub(crate) window_properties: Option<super::window_properties::ResolvedWindowProperties>,
    pub(crate) window_properties_error: Option<String>,
    pub(crate) geometry_stale: bool,
    pub(crate) outputs: OutputSender,
    pub(crate) error: Option<String>,
    pub(crate) animation_request: Option<u64>,
    pub(crate) visible: bool,
    pub(crate) painted_focus: Option<super::focus::ComponentInstanceId>,
    pub(crate) painted_hover: Option<super::focus::ComponentInstanceId>,
    pub(crate) pointer_position: Option<super::geometry::Point>,
    pub(crate) overlay: super::performance_overlay::PerformanceOverlay,
    /// Unadorned last successful render, reused for diagnostic-only refreshes.
    pub(crate) last_render: Option<super::display_list::RenderOutput>,
    pub(crate) diagnostics_dirty: bool,
}

impl UiInstance {
    /// Scope a published component's structural path to its definition.
    pub fn component_id(
        &self,
        index: usize,
    ) -> pixui_base::PixuiResult<super::focus::ComponentInstanceId> {
        let path = self
            .layout
            .component_paths
            .get(index)
            .ok_or_else(|| pixui_base::pixui_error!("unknown component index"))?;
        Ok(super::focus::ComponentInstanceId {
            definition: self.definition,
            path: path.clone(),
        })
    }

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
    /// Most recent metadata resolution error, separate from painting failures.
    pub fn window_properties_error(&self) -> Option<&str> {
        self.window_properties_error.as_deref()
    }
    pub fn last_error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    /// Effective scroll for this viewport, derived from shared definition state.
    pub fn scroll_offset(&self) -> f32 {
        self.layout.scroll_offset
    }
}
