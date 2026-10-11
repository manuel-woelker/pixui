//! Component painters generate commands on the worker; the native backend paints pixels.

use super::context::PaintContext;
use crate::live_model::component::Component;
use pixui_base::PixuiResult;

/// One painter per component per application. Paint only reads props/state;
/// local updates and application actions belong to separate callbacks.
/// Commands use local logical coordinates and are clipped to the supplied size.
pub trait Painter<C: Component>: Send + 'static {
    /// Default registration retains a raw configurable painter. Standard painters
    /// override this to install their reflected slots and external defaults.
    fn register(
        self,
        registry: &mut super::registry::PainterRegistry,
        components: &crate::component_registry::registry::ComponentRegistry,
        _styles: &mut crate::styles::registry::StyleRegistry,
    ) -> PixuiResult<()>
    where
        Self: Sized,
    {
        registry.register::<C>(components, self)
    }

    /// Pure semantic state for style resolution. Engine hover is added after layout.
    fn style_states(_props: &C::Props, _state: &C::State) -> crate::styles::style::StyleStates {
        Default::default()
    }
    /// Returns finite nonnegative content size. May run repeatedly; no I/O,
    /// mutation, atlas allocation, actions, or animation requests are permitted.
    fn measure(
        &self,
        context: &super::measure::MeasureContext<'_, C>,
    ) -> PixuiResult<crate::ui::geometry::Size>;
    /// Editable painters provide caret geometry using the same font as paint.
    /// Other components return None. This probe has the same purity as measure.
    fn text_input_geometry(
        &self,
        _context: &super::measure::MeasureContext<'_, C>,
    ) -> PixuiResult<Option<crate::ui::text_input::geometry::TextInputGeometry>> {
        Ok(None)
    }
    fn paint(&self, context: &mut PaintContext<'_, C>) -> PixuiResult<()>;
}
