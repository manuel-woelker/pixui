use crate::app::Application;
use crate::draw::draw_list::DrawList;
use crate::viewport::Viewport;
use pixui_base::result::PixuiResult;

/// Renders a named component into a draw list for a viewport.
pub trait ComponentDrawRenderer: Send {
    /// Builds the draw list for a component using the current application state.
    fn render(&self, application: &Application, viewport: &Viewport) -> PixuiResult<DrawList>;
}

impl<F> ComponentDrawRenderer for F
where
    F: for<'a, 'b> Fn(&'a Application, &'b Viewport) -> PixuiResult<DrawList> + Send,
{
    fn render(&self, application: &Application, viewport: &Viewport) -> PixuiResult<DrawList> {
        self(application, viewport)
    }
}
