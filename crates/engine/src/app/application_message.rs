use pixui_base::result::PixuiResult;
use crate::app::Application;

pub enum ApplicationMessage {
    RunOnce(Box<dyn FnOnce(&mut Application) -> PixuiResult<()> + Send>),
}