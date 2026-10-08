//! A live component gallery: props read shared data, activations dispatch actions.
use crate::model::data::Sample;
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    components::{
        button::ButtonProps, checkbox::CheckboxProps, image::ImageProps, label::LabelProps,
    },
    expression::{context::ExpressionContext, expression::Expression},
    layout::{container::ContainerPart, grid::Track, style::LayoutStyle},
    live_model::{
        match_part::{MatchCandidate, MatchPart, MatchPattern},
        part::{ComponentPart, ForLoopPart, LivePart},
    },
    painters::standard::StandardComponents,
    resources::path::ResourcePath,
    ui::{
        activation::ActionBinding, definition::UiDefinition, presentation::PresentationSettings,
        window_properties::WindowProperties,
    },
};

/// Read a declared value with a useful error instead of an unchecked downcast.
fn text(values: &[pixui_reflect::DynamicObject<'_>], index: usize) -> PixuiResult<String> {
    values
        .get(index)
        .and_then(|value| value.downcast_ref::<String>())
        .cloned()
        .ok_or_else(|| pixui_error!("showcase requires a string expression"))
}
fn label(components: StandardComponents, expression: Expression) -> LivePart {
    LivePart::Component(ComponentPart::typed_with_expressions(
        components.label,
        vec![expression],
        |_, _, values| {
            Ok(LabelProps {
                text: text(values, 0)?,
            })
        },
    ))
}
fn button(
    components: StandardComponents,
    source: &str,
    activate: pixui_engine::ui::activation::ActivationFactory,
) -> PixuiResult<LivePart> {
    Ok(LivePart::Component(
        ComponentPart::typed_with_expressions(
            components.button,
            vec![Expression::text(source)?],
            |_, _, values| {
                Ok(ButtonProps {
                    label: text(values, 0)?,
                })
            },
        )
        .with_activation(activate),
    ))
}
fn checkbox(
    components: StandardComponents,
    source: &str,
    checked: Expression,
    activate: pixui_engine::ui::activation::ActivationFactory,
) -> PixuiResult<LivePart> {
    Ok(LivePart::Component(
        ComponentPart::typed_with_expressions(
            components.checkbox,
            vec![Expression::text(source)?, checked],
            |_, _, values| {
                Ok(CheckboxProps {
                    label: text(values, 0)?,
                    checked: *values
                        .get(1)
                        .and_then(|value| value.downcast_ref::<bool>())
                        .ok_or_else(|| pixui_error!("showcase requires a boolean expression"))?,
                })
            },
        )
        .with_activation(activate),
    ))
}

/// One definition with explicitly declared messages, shared by every presentation.
pub fn definition(
    app: &ApplicationHandle,
    components: StandardComponents,
) -> PixuiResult<UiDefinition> {
    let slice = app.inspect(|app| Ok(app.slice_named("showcase")?.id()))?;
    let details = app.entity_ref::<bool>(slice, "details")?;
    let checked = app.entity_ref::<bool>(slice, "checked")?;
    let count = app.entity_ref::<u64>(slice, "count")?;
    let samples = app.collection_key("showcase", "samples")?;
    let mut parts = vec![
        label(components, Expression::text("PixUI · Widget showcase")?),
        label(
            components,
            Expression::i18n("Counter: {count}", [("count", Expression::entity(count))])?,
        ),
        button(components, "Increment counter", increment)?,
        button(components, "Reset counter", reset)?,
        checkbox(
            components,
            "An independent checkbox",
            Expression::entity(checked),
            toggle_checked,
        )?,
        checkbox(
            components,
            "Show details",
            Expression::entity(details),
            toggle_details,
        )?,
        LivePart::Match(MatchPart::new(
            Expression::entity(details),
            vec![MatchCandidate {
                pattern: MatchPattern::value(true),
                part: label(
                    components,
                    Expression::text(
                        "Shared data, hover and focus · independent theme and locale",
                    )?,
                ),
            }],
        )?),
        LivePart::Component(
            ComponentPart::typed(components.image, |_, _| {
                ImageProps::new("images/pixui-logo.png")
            })
            .with_layout(LayoutStyle::fixed(72.0, 72.0)),
        ),
        button(components, "Add collection row", add_sample)?,
        LivePart::ForLoop(ForLoopPart {
            expression: Expression::from_collection(samples),
            body: Box::new(LivePart::Component(ComponentPart::typed(
                components.label,
                |context, _| {
                    let sample = context
                        .value()?
                        .downcast_ref::<Sample>()
                        .ok_or_else(|| pixui_error!("sample row requires Sample context"))?;
                    Ok(LabelProps {
                        text: sample.label.clone(),
                    })
                },
            ))),
        }),
        label(
            components,
            Expression::text("Tab: focus · Enter/Space: activate · F11: performance")?,
        ),
    ];
    // Align the two actions in equal-width Grid columns; the enclosing column
    // and all painters use the same layout path.
    let actions = ContainerPart::grid()
        .with_columns(vec![Track::fraction(1.0), Track::fraction(1.0)])
        .with_gap(8.0)
        .with_children(parts.drain(2..4).collect())
        .into();
    parts.insert(2, actions);
    let root = ContainerPart::column()
        .with_gap(8.0)
        .with_children(parts)
        .into();
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

fn binding(context: &ExpressionContext<'_>, name: &'static str) -> PixuiResult<ActionBinding> {
    let app = context.application()?;
    let action = app.slice_named("showcase")?.action_handle_named(name)?;
    Ok(Box::new(move |_| action.call(vec![])))
}

fn increment(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    binding(context, "increment")
}
fn reset(context: &ExpressionContext<'_>, _: &PresentationSettings) -> PixuiResult<ActionBinding> {
    binding(context, "reset")
}
fn toggle_checked(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    binding(context, "toggle_checked")
}
fn toggle_details(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    binding(context, "toggle_details")
}
fn add_sample(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    binding(context, "add_sample")
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
