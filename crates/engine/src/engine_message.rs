use crate::engine_state::EngineState;
use pixui_base::result::PixuiResult;

pub type EngineRunOnce = Box<dyn FnOnce(&mut EngineState) -> PixuiResult<()> + Send>;

pub enum EngineMessage {
    RunOnce(EngineRunOnce),
}
