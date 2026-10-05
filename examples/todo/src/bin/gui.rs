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
use pixui_gui::host::{self, WindowSpec};

fn main() -> PixuiResult<()> {
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
    host::run(application, windows)
}
