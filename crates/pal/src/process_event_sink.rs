use crate::process_event::ProcessEvent;
use pixui_base::result::PixuiResult;

/// Receives child process lifecycle events during execution.
pub trait ProcessEventSink {
    /// Handles one process event.
    fn handle_event(&mut self, event: ProcessEvent) -> PixuiResult<()>;
}
