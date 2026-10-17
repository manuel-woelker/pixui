//! Explicit application exports shared by the showcase's source templates.
use crate::model::data::{LayoutProject, LayoutTask, NavigationEntry, NavigationState, Sample};
use pixui_base::PixuiResult;
use pixui_engine::{
    application::app::Application,
    layout::style::{LayoutStyle, Length},
    painters::standard::StandardComponents,
    styles::{family::StyleAssignment, slots::SlotAssignment},
    ui_language::resolution::environment::Environment,
};

pub(crate) fn create(
    app: &Application,
    components: StandardComponents,
) -> PixuiResult<Environment> {
    let slice = app.slice_named("showcase")?;
    let mut env = Environment::new();
    env.component("Label", components.label)?;
    env.component("Button", components.button)?;
    env.component("Checkbox", components.checkbox)?;
    env.component("TextInput", components.text_input)?;
    env.component("Image", components.image)?;
    env.component(
        "ClickArea",
        app.components()
            .component::<crate::click_area::ClickArea>()?,
    )?;
    for name in ["last_click", "text", "uppercase", "limited"] {
        env.entity(name, app, app.entity_ref::<String>(slice.id(), name)?)?;
    }
    env.entity("count", app, app.entity_ref::<u64>(slice.id(), "count")?)?;
    for name in ["details", "checked"] {
        env.entity(name, app, app.entity_ref::<bool>(slice.id(), name)?)?;
    }
    env.entity(
        "navigation",
        app,
        app.entity_ref::<NavigationState>(slice.id(), "navigation")?,
    )?;
    env.reflected_type::<NavigationState>();
    env.reflected_type::<Vec<NavigationEntry>>();
    env.value_type::<NavigationEntry>();
    env.entity(
        "projects",
        app,
        app.entity_ref::<Vec<LayoutProject>>(slice.id(), "layout_projects")?,
    )?;
    env.reflected_type::<LayoutProject>();
    env.reflected_type::<LayoutTask>();
    env.reflected_type::<Vec<LayoutTask>>();
    env.collection::<Sample>("samples", app, slice.collection_index("samples")?)?;
    for name in [
        "increment",
        "reset",
        "select",
        "add_sample",
        "toggle_checked",
        "toggle_details",
        "record_click",
        "edit_text",
        "edit_uppercase",
        "edit_limited",
    ] {
        env.action(app, "showcase", name, slice.action_handle_named(name)?)?;
    }
    env.layout(
        "full",
        LayoutStyle {
            width: Length::Percent(1.0),
            min_width: Length::Pixels(0.0),
            ..Default::default()
        },
    )?;
    env.layout("logo", LayoutStyle::fixed(160.0, 160.0))?;
    env.layout("click_area", LayoutStyle::fixed(300.0, 40.0))?;
    let primary = StyleAssignment::new(app.styles().family("showcase.primary")?);
    for (name, assignment) in [
        ("primary", primary.clone()),
        (
            "quiet",
            StyleAssignment::new(app.styles().family("showcase.quiet")?),
        ),
        (
            "danger",
            StyleAssignment::new(app.styles().family("showcase.danger")?),
        ),
        ("compact", primary.clone().variant("compact")),
        ("mono", primary.clone().variant("mono")),
        ("purple", primary.patch(crate::styles::local_color())),
    ] {
        env.styles(name, vec![SlotAssignment::new("body", assignment)])?;
    }
    Ok(env)
}
