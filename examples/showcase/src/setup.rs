//! Reusable startup for the native executable and worker-side tests.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::{app::Application, application_handle::ApplicationHandle},
    resources::{directory::DirectoryFilesystem, image_loader::ImageLoader},
    ui::definition::UiDefinitionId,
};
use std::sync::Arc;

/// Build the gallery with standard painters and a repository-relative asset root.
/// No native windows are created here; tests can inspect render outputs directly.
pub fn create() -> PixuiResult<(ApplicationHandle, UiDefinitionId)> {
    let application = Application::new();
    crate::model::register(&application)?;
    application.set_image_loader(ImageLoader::new(Arc::new(DirectoryFilesystem::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"),
    )?)))?;
    let components = application.register_standard_components()?;
    application.register_standard_painters()?;
    let definition =
        application.register_ui(crate::gallery::definition(&application, components)?)?;
    Ok((application, definition))
}
