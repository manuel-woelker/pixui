use pixui_base::result::PixuiResult;
use crate::app::{Application, ApplicationHandle};

/// Core engine entry point for the pixui project.
pub struct Engine {
    application: ApplicationHandle,
}

impl Engine {
    /// Creates a new engine instance.
    pub fn new() -> PixuiResult<Self> {
        Ok(Self {
            application: Application::spawn()?,
        })
    }

}

#[cfg(test)]
mod tests {

}
