//! Two native windows sharing a todo UI definition and application data.

use pixui_example_todo::{custom_button_painter::CustomButtonPainter, gui_ui, todo};

use pixui_base::PixuiResult;
use pixui_engine::{
    application::app::Application,
    ui::{
        geometry::Size,
        presentation::{PresentationSettings, Theme},
    },
};
use pixui_gui::{
    host::{self, WindowSpec},
    renderer::factory::RendererSelection,
};

fn main() -> PixuiResult<()> {
    let mut selection = RendererSelection::Auto;
    let mut freeze_animation = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--renderer" {
            selection = args
                .next()
                .ok_or_else(|| {
                    pixui_base::pixui_error!("--renderer requires auto, software, or femtovg")
                })?
                .parse()?;
        } else if arg == "--freeze-animation" {
            freeze_animation = true;
        } else if arg != "--custom-painter" {
            return Err(pixui_base::pixui_error!("unknown argument `{arg}`"));
        }
    }
    let application = Application::new();
    application.add_slice(todo::create_slice()?)?;
    let actions = todo::actions::TodoActions::bind(&application)?;
    actions.add_todo("Create a todo application")?;
    actions.add_todo("Try dark mode / Dunkelmodus ausprobieren")?;
    let components = application.register_standard_components()?;
    if std::env::args().any(|arg| arg == "--custom-painter") {
        application.register_painter::<pixui_engine::components::button::ButtonComponent>(
            CustomButtonPainter,
        )?;
        application.register_painter::<pixui_engine::components::label::LabelComponent>(
            pixui_engine::painters::label::LabelPainter,
        )?;
        application.register_painter::<pixui_engine::components::checkbox::CheckboxComponent>(
            pixui_engine::painters::checkbox::CheckboxPainter,
        )?;
    } else {
        application.register_standard_painters()?;
    }
    let definition = application.register_ui(gui_ui::definition(
        &application,
        components,
        pixui_example_todo::orbiting_comets::register(&application)?,
    )?)?;
    let mut windows = Vec::new();
    for (title, theme, locale, width, height) in [
        ("Todos - English / light", Theme::Light, "en", 640.0, 480.0),
        ("Todos - Deutsch / dark", Theme::Dark, "de", 420.0, 640.0),
    ] {
        let settings = PresentationSettings {
            theme,
            timestamp_us: freeze_animation.then_some(0),
            locale: locale.into(),
            viewport: Size { width, height },
            ..Default::default()
        };
        let (instance, outputs) = application.create_ui(definition, settings.clone())?;
        windows.push(WindowSpec {
            title: title.into(),
            instance,
            outputs,
            settings,
        });
    }
    host::run_with_renderer(application, windows, selection)
}
