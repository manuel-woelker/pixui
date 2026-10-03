use super::application_slice::ApplicationSlice;

/// An application's ordered slices of data.
#[derive(Default)]
pub struct Application {
    pub slices: Vec<ApplicationSlice>,
}

impl Application {
    /// Creates an application with no slices.
    pub fn new() -> Self {
        Self::default()
    }
}
