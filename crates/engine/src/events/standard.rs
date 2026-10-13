//! Event interfaces shared by standard components.
use pixui_base::PixuiResult;
use pixui_engine_macros::component_events;

#[component_events(crate)]
pub trait ActivationEvents {
    fn activate(&self) -> PixuiResult<()>;
}
#[component_events(crate)]
pub trait TextInputEvents {
    fn change(&self, content: String) -> PixuiResult<()>;
    fn submit(&self) -> PixuiResult<()>;
}
