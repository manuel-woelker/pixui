//! Explicit Rust exports connect a .pixui template to registered application data.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    live_model::part::LivePart,
    painters::standard::StandardComponents,
    ui_language::{compiler::compile, resolution::environment::Environment},
};
pub fn page(
    application: &ApplicationHandle,
    components: StandardComponents,
) -> PixuiResult<LivePart> {
    application.inspect(move |app| {
        let slice = app.slice_named("showcase")?;
        let mut environment = Environment::new();
        environment.component("Label", components.label)?;
        environment.component("Button", components.button)?;
        environment.component("Checkbox", components.checkbox)?;
        environment.component(
            "ClickArea",
            app.components()
                .component::<crate::click_area::ClickArea>()?,
        )?;
        environment.entity(
            "last_click",
            app,
            app.entity_ref::<String>(slice.id(), "last_click")?,
        )?;
        environment.layout(
            "click_area",
            pixui_engine::layout::style::LayoutStyle::fixed(300.0, 40.0),
        )?;
        environment.entity("count", app, app.entity_ref::<u64>(slice.id(), "count")?)?;
        environment.entity(
            "details",
            app,
            app.entity_ref::<bool>(slice.id(), "details")?,
        )?;
        environment.collection::<crate::model::data::Sample>(
            "samples",
            app,
            slice.collection_index("samples")?,
        )?;
        for name in ["increment", "add_sample", "toggle_details", "record_click"] {
            environment.action(app, "showcase", name, slice.action_handle_named(name)?)?;
        }
        compile(
            "examples/showcase/ui/language.pixui",
            include_str!("../ui/language.pixui"),
            &environment,
        )
        .map(|compiled| compiled.into_template())
        .map_err(|errors| pixui_error!("{errors}"))
    })
}

/// Controlled props and event payloads compiled from source; no handwritten change factories.
pub fn text_page(
    application: &ApplicationHandle,
    components: StandardComponents,
) -> PixuiResult<LivePart> {
    application.inspect(move |app| {
        let slice = app.slice_named("showcase")?;
        let mut environment = Environment::new();
        environment.component("Label", components.label)?;
        environment.component("Button", components.button)?;
        environment.component("TextInput", components.text_input)?;
        for name in ["text", "uppercase", "limited"] {
            environment.entity(name, app, app.entity_ref::<String>(slice.id(), name)?)?;
        }
        environment.entity("count", app, app.entity_ref::<u64>(slice.id(), "count")?)?;
        for name in ["edit_text", "edit_uppercase", "edit_limited"] {
            environment.action(app, "showcase", name, slice.action_handle_named(name)?)?;
        }
        environment.layout(
            "full",
            pixui_engine::layout::style::LayoutStyle {
                width: pixui_engine::layout::style::Length::Percent(1.0),
                min_width: pixui_engine::layout::style::Length::Pixels(0.0),
                ..Default::default()
            },
        )?;
        compile(
            "examples/showcase/ui/text.pixui",
            include_str!("../ui/text.pixui"),
            &environment,
        )
        .map(|compiled| compiled.into_template())
        .map_err(|errors| pixui_error!("{errors}"))
    })
}

/// The core image component retains validated resource-path props in source definitions.
pub fn images_page(
    application: &ApplicationHandle,
    components: StandardComponents,
) -> PixuiResult<LivePart> {
    application.inspect(move |_app| {
        let mut environment = Environment::new();
        environment.component("Label", components.label)?;
        environment.component("Image", components.image)?;
        environment.layout(
            "logo",
            pixui_engine::layout::style::LayoutStyle::fixed(160.0, 160.0),
        )?;
        environment.layout(
            "full",
            pixui_engine::layout::style::LayoutStyle {
                width: pixui_engine::layout::style::Length::Percent(1.0),
                min_width: pixui_engine::layout::style::Length::Pixels(0.0),
                ..Default::default()
            },
        )?;
        compile(
            "examples/showcase/ui/images.pixui",
            include_str!("../ui/images.pixui"),
            &environment,
        )
        .map(|compiled| compiled.into_template())
        .map_err(|errors| pixui_error!("{errors}"))
    })
}
