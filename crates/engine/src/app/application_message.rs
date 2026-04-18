use crate::app::Application;
use pixui_base::result::PixuiResult;

pub type ApplicationRunOnce = Box<dyn FnOnce(&mut Application) -> PixuiResult<()> + Send>;

pub enum ApplicationMessage {
    RunOnce(ApplicationRunOnce),
}
