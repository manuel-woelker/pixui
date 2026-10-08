//! Component painters generate commands on the worker; the native backend paints pixels.

use super::context::PaintContext;
use crate::live_model::component::Component;
use pixui_base::PixuiResult;

/// One painter per component per application. Paint only reads props/state;
/// local updates and application actions belong to separate callbacks.
/// Commands use local logical coordinates and are clipped to the supplied size.
pub trait Painter<C: Component>: Send + 'static {
    /// Returns finite nonnegative content size. May run repeatedly; no I/O,
    /// mutation, atlas allocation, actions, or animation requests are permitted.
    fn measure(
        &self,
        context: &super::measure::MeasureContext<'_, C>,
    ) -> PixuiResult<crate::ui::geometry::Size>;
    fn paint(&self, context: &mut PaintContext<'_, C>) -> PixuiResult<()>;
}
