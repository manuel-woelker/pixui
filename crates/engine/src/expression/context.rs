use crate::application::app::Application;

/// Read-only application state available during expression evaluation.
///
/// For a running application, create this context inside an inspection callback.
/// Evaluated values borrow the application and cannot escape that callback.
pub struct ExpressionContext<'a> {
    application: &'a Application,
}

impl<'a> ExpressionContext<'a> {
    pub fn new(application: &'a Application) -> Self {
        Self { application }
    }

    pub fn application(&self) -> &'a Application {
        self.application
    }
}
