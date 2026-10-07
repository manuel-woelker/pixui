//! Todo-domain catalogs. User-created todo titles remain application data.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    i18n::{format::TranslationFormat, po::PoFormat},
};

pub fn configure(application: &ApplicationHandle) -> PixuiResult<()> {
    let de = application.register_language("de")?;
    application.install_translations(
        "todos",
        de,
        PoFormat.import(include_str!("../translations/todos/de.po"))?,
    )?;
    Ok(())
}

/// Start optional image/catalog watching. Retain the guard until native shutdown.
pub fn hot_reload(
    application: &ApplicationHandle,
) -> PixuiResult<pixui_engine::resources::reload::session::ResourceReloadSession> {
    let german = application.register_language("de")?;
    let session =
        pixui_engine::resources::reload::builder::ResourceReloadBuilder::new(application.clone())
            .watch_images()
            .catalog(
                std::sync::Arc::new(
                    pixui_engine::resources::directory::DirectoryFilesystem::new(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/translations"
                    ))?,
                ),
                pixui_engine::resources::path::ResourcePath::new("todos/de.po")?,
                "todos",
                german,
                std::sync::Arc::new(PoFormat),
            )?
            .start()?;
    session.wait_initial(std::time::Duration::from_secs(5))?;
    Ok(session)
}
