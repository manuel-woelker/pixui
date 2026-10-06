//! Window presentation plugins, used exclusively on the GUI thread.

use pixui_base::PixuiResult;
use pixui_engine::ui::display_list::DisplayList;
use std::sync::Arc;
use winit::window::Window;

/// Submission succeeded, or presentation was temporarily unavailable.
/// Only Presented permits the host to advance its input revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderOutcome {
    Presented,
    Skipped,
}

pub use pixui_engine::ui::performance::RendererTimings;

/// Executes immutable display lists and owns one window's presentation resources.
/// Renderers need not be Send. Coordinates are logical pixels; dimensions are
/// physical pixels. Suspend must release native surfaces. Render must validate
/// inputs before submission; Skipped requests a bounded host retry.
pub trait Renderer {
    fn resize(&mut self, width: u32, height: u32, scale: f32) -> PixuiResult<()>;
    fn render(&mut self, display: &DisplayList) -> PixuiResult<RenderOutcome>;
    /// Optional CPU diagnostics. Custom renderers can leave this unavailable.
    fn timings(&self) -> Option<RendererTimings> {
        None
    }
    fn suspend(&mut self);
    fn resume(&mut self) -> PixuiResult<()>;
}

/// Creates independently owned renderers on the event-loop thread. A factory
/// may share GPU device/queue state without sharing window surfaces.
pub trait RendererFactory {
    fn create(&mut self, window: Arc<Window>) -> PixuiResult<Box<dyn Renderer>>;
}
