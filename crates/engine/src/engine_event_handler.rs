use crate::engine_event_context::EngineEventContext;
use pixui_base::result::PixuiResult;

/// Handles events submitted to an engine.
pub trait EngineEventHandler<E>: Send {
    /// Handles a submitted event.
    fn handle_event(&mut self, context: &mut EngineEventContext<'_, E>) -> PixuiResult<()>;
}

impl<E, F> EngineEventHandler<E> for F
where
    E: 'static,
    F: FnMut(&mut EngineEventContext<'_, E>) -> PixuiResult<()> + Send,
{
    fn handle_event(&mut self, context: &mut EngineEventContext<'_, E>) -> PixuiResult<()> {
        self(context)
    }
}
