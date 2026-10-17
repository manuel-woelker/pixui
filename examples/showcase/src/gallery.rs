//! Compile the gallery and bridge the remaining host-only definition features.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    components::button::ButtonProps,
    expression::{context::ExpressionContext, expression::Expression},
    live_model::part::{ComponentPart, LivePart},
    painters::standard::StandardComponents,
    resources::path::ResourcePath,
    ui::{
        activation::ActionBinding, definition::UiDefinition, presentation::PresentationSettings,
        window_properties::WindowProperties,
    },
    ui_language::compiler::compile,
};

/// UI declarations live in source templates; Rust supplies exports, composition,
/// dynamic styles and native metadata until those have language support.
pub fn definition(
    app: &ApplicationHandle,
    components: StandardComponents,
) -> PixuiResult<UiDefinition> {
    register_pages(app, components)?;
    let root = app.inspect(move |app| {
        let env = crate::environment::create(app, components)?;
        Ok(compile(
            "examples/showcase/ui/showcase.pixui",
            include_str!("../ui/showcase.pixui"),
            &env,
        )
        .map_err(|errors| pixui_error!("{errors}"))?
        .into_template())
    })?;
    let slice = app.inspect(|app| Ok(app.slice_named("showcase")?.id()))?;
    let count = app.entity_ref::<u64>(slice, "count")?;
    Ok(UiDefinition::new("showcase", root)
        .with_translation_domain("showcase")
        .with_window_property_expressions(
            vec![Expression::i18n(
                "PixUI Showcase — {count}",
                [("count", Expression::entity(count))],
            )?],
            window_properties,
        ))
}

/// Compile all views once, including inactive ones. The UI has no duplicated
/// page names or page-count assumptions; this is the single navigation list.
fn register_pages(app: &ApplicationHandle, components: StandardComponents) -> PixuiResult<()> {
    use crate::model::{actions::ShowcaseActions, data::NavigationEntry};
    use pixui_engine::ui_language::{bundle::BundleLimits, source::bundle::SourceBundle};
    let env = app.inspect(move |app| crate::environment::create(app, components))?;
    let pages = [
        (
            0,
            "Buttons",
            "examples/showcase/ui/buttons.pixui",
            include_str!("../ui/buttons.pixui"),
        ),
        (
            1,
            "Checkboxes",
            "examples/showcase/ui/checkboxes.pixui",
            include_str!("../ui/checkboxes.pixui"),
        ),
        (
            2,
            "Images",
            "examples/showcase/ui/images.pixui",
            include_str!("../ui/images.pixui"),
        ),
        (
            3,
            "Collections",
            "examples/showcase/ui/collections.pixui",
            include_str!("../ui/collections.pixui"),
        ),
        (
            4,
            "Text and translations",
            "examples/showcase/ui/text.pixui",
            include_str!("../ui/text.pixui"),
        ),
        (
            5,
            "Styles",
            "examples/showcase/ui/styles.pixui",
            include_str!("../ui/styles.pixui"),
        ),
        (
            6,
            "Layout",
            "examples/showcase/ui/layout.pixui",
            include_str!("../ui/layout.pixui"),
        ),
        (
            7,
            "UI language",
            "examples/showcase/ui/language.pixui",
            include_str!("../ui/language.pixui"),
        ),
    ];
    let mut sources = SourceBundle::new();
    for (_, _, id, text) in pages {
        sources.add(id, text)?;
    }
    let installed = app.install_bundle(
        sources,
        pages.iter().map(|(_, _, id, _)| (*id).to_owned()).collect(),
        env,
        "showcase",
        BundleLimits::default(),
    )?;
    let mut entries = Vec::new();
    for (id, title, module, _) in pages {
        let mut component = installed.component(module)?;
        if id == 5 {
            let mut styles = app.inspect(move |app| app.clone_template(component))?;
            let LivePart::Container(container) = &mut styles else {
                return Err(pixui_error!("styles template must have a container root"));
            };
            container.children.push(dynamic_button(components)?);
            component = app.register_template("styles-with-dynamic-example", "showcase", styles)?;
        }
        entries.push(NavigationEntry {
            id,
            title: app.localized_text("showcase", title)?,
            component,
        });
    }
    ShowcaseActions::bind(app)?.install_navigation(entries)?;
    Ok(())
}

fn dynamic_button(components: StandardComponents) -> PixuiResult<LivePart> {
    Ok(ComponentPart::typed_with_expressions(
        components.button,
        vec![Expression::text("Dynamic style variant")?],
        |_, _, values| {
            Ok(ButtonProps {
                label: text(values, 0)?,
            })
        },
    )
    .with_style_resolver(dynamic_style)
    .with_activation(increment)
    .into())
}
fn increment(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    let action = context
        .application()?
        .slice_named("showcase")?
        .action_handle_named("increment")?;
    Ok(Box::new(move |_| action.call(vec![])))
}

/// Read a declared value with a useful error instead of an unchecked downcast.
fn text(values: &[pixui_reflect::DynamicObject<'_>], index: usize) -> PixuiResult<String> {
    values
        .get(index)
        .and_then(|value| value.downcast_ref::<String>())
        .cloned()
        .ok_or_else(|| pixui_error!("showcase requires a string expression"))
}
fn window_properties(
    _: &ExpressionContext<'_>,
    _: &PresentationSettings,
    values: &[pixui_reflect::DynamicObject<'_>],
) -> PixuiResult<WindowProperties> {
    Ok(WindowProperties {
        title: text(values, 0)?.into(),
        icon: Some(ResourcePath::new("images/pixui-logo.png")?),
    })
}

fn dynamic_style(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<Vec<pixui_engine::styles::slots::SlotAssignment>> {
    let app = context.application()?;
    let slice = app.slice_named("showcase")?;
    let checked = *app.entity::<bool>(slice.id(), "checked")?;
    let mut assignment = pixui_engine::styles::family::StyleAssignment::new(
        app.styles().family("showcase.primary")?,
    );
    if checked {
        assignment = assignment.variant("compact");
    }
    Ok(vec![pixui_engine::styles::slots::SlotAssignment::new(
        "body", assignment,
    )])
}
