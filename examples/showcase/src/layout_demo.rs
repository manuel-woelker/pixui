//! Nested Grid and Flex composition compiled from source, including looped cards.
use crate::model::data::{LayoutProject, LayoutTask};
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::{application_handle::ApplicationHandle, object_ref::ObjectRef},
    live_model::part::LivePart,
    painters::standard::StandardComponents,
    ui_language::{compiler::compile, resolution::environment::Environment},
};

/// One card expands into 36 physical cards, each with six task rows. Keep this
/// intentionally substantial example to exercise retained layout and painting.
pub(crate) fn page(
    application: &ApplicationHandle,
    components: StandardComponents,
    count: ObjectRef<u64>,
    projects: ObjectRef<Vec<LayoutProject>>,
) -> PixuiResult<LivePart> {
    application.inspect(move |app| {
        let mut environment = Environment::new();
        environment.component("Label", components.label)?;
        environment.component("Button", components.button)?;
        environment.entity("count", app, count)?;
        environment.entity("projects", app, projects)?;
        environment.reflected_type::<LayoutProject>();
        environment.reflected_type::<LayoutTask>();
        environment.reflected_type::<Vec<LayoutTask>>();
        let slice = app.slice_named("showcase")?;
        for name in ["increment", "reset"] {
            environment.action(app, "showcase", name, slice.action_handle_named(name)?)?;
        }
        compile(
            "examples/showcase/ui/layout.pixui",
            include_str!("../ui/layout.pixui"),
            &environment,
        )
        .map(|compiled| compiled.into_template())
        .map_err(|errors| pixui_error!("{errors}"))
    })
}
