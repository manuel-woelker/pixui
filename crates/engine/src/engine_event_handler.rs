use crate::app::Application;
use crate::engine_event_context::EngineEventContext;
use pixui_base::result::PixuiResult;

/// Handles events submitted to an engine.
pub trait EngineEventHandler: Send {
    /// Event type handled by this handler.
    type Event;

    /// Handles a submitted event.
    fn handle_event(
        &mut self,
        application: &mut Application,
        context: &mut EngineEventContext<Self::Event>,
    ) -> PixuiResult<()>;
}
