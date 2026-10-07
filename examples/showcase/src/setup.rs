//! Reusable startup for the native executable and worker-side tests.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::{app::Application, application_handle::ApplicationHandle},
    resources::{directory::DirectoryFilesystem, image_loader::ImageLoader},
    ui::definition::UiDefinitionId,
};
use std::sync::Arc;

/// Register declarations and translations without opening any filesystem resources.
/// No native windows are created here; tests can inspect render outputs directly.
pub fn register() -> PixuiResult<(ApplicationHandle, UiDefinitionId)> {
    let application = Application::new();
    crate::model::register(&application)?;
    let components = application.register_standard_components()?;
    application.register_standard_painters()?;
    let definition =
        application.register_ui(crate::gallery::definition(&application, components)?)?;
    let de = application.register_language("de")?;
    let catalog = pixui_engine::i18n::format::TranslationFormat::import(
        &pixui_engine::i18n::po::PoFormat,
        include_str!("../translations/showcase/de.po"),
    )?;
    application.install_translations("showcase", de, catalog)?;
    Ok((application, definition))
}

/// Configure filesystem resources for interactive rendering; export uses register.
pub fn create() -> PixuiResult<(ApplicationHandle, UiDefinitionId)> {
    let (application, definition) = register()?;
    application.set_image_loader(ImageLoader::new(Arc::new(DirectoryFilesystem::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"),
    )?)))?;
    Ok((application, definition))
}
