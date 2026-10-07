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
