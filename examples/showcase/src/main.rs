//! Two presentations of the same interactive widget gallery.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::ui::{
    geometry::Size,
    presentation::{PresentationSettings, Theme},
};
use pixui_gui::{
    host::{self, WindowSpec},
    renderer::factory::RendererSelection,
};

fn main() -> PixuiResult<()> {
    let mut export = None;
    let mut renderer = RendererSelection::Auto;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--export-translations" => {
                export = Some(args.next().ok_or_else(|| {
                    pixui_error!("--export-translations requires an output path")
                })?);
            }
            "--renderer" => {
                renderer = args
                    .next()
                    .ok_or_else(|| pixui_error!("--renderer requires auto, software, or femtovg"))?
                    .parse()?;
            }
            _ => return Err(pixui_error!("unknown argument `{arg}`")),
        }
    }
    if let Some(path) = export {
        let (application, _) = pixui_example_showcase::setup::register()?;
        let catalog =
            application.export_translations("showcase", &pixui_engine::i18n::po::PoFormat)?;
        std::fs::write(path, catalog)
            .map_err(|error| pixui_error!("write POT catalog: {error}"))?;
        return Ok(());
    }
    let (application, definition) = pixui_example_showcase::setup::create()?;
    let mut windows = Vec::new();
    for (theme, locale) in [(Theme::Light, "en"), (Theme::Dark, "de")] {
        let settings = application.presentation_language(
            PresentationSettings {
                theme,
                locale: locale.into(),
                viewport: Size {
                    width: 760.0,
                    height: 720.0,
                },
                ..Default::default()
            },
            locale,
        )?;
        let (instance, outputs) = application.create_ui(definition, settings.clone())?;
        windows.push(WindowSpec {
            title: "PixUI Showcase".into(),
            instance,
            outputs,
            settings,
        });
    }
    host::run_with_renderer(application, windows, renderer)
}
