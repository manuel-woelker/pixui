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
        for name in ["increment", "add_sample", "toggle_details"] {
            environment.action(name, slice.action_handle_named(name)?)?;
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
