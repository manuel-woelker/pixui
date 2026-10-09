//! A live component gallery: props read shared data, activations dispatch actions.
use crate::model::data::Sample;
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    components::{
        button::ButtonProps, checkbox::CheckboxProps, image::ImageProps, label::LabelProps,
    },
    expression::{context::ExpressionContext, expression::Expression},
    layout::{
        container::ContainerPart,
        grid::Track,
        style::{LayoutStyle, Length},
    },
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

const SIDEBAR_WIDTH: f32 = 200.0;

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
    let selected = app.entity_ref::<u64>(slice, "selected")?;
    let buttons = page(vec![
        label(components, Expression::text("Buttons")?),
        label(
            components,
            Expression::i18n("Counter: {count}", [("count", Expression::entity(count))])?,
        ),
        ContainerPart::row()
            .with_gap(8.0)
            .with_children(vec![
                button(components, "Increment counter", increment)?,
                button(components, "Reset counter", reset)?,
            ])
            .into(),
    ]);
    let checkboxes = page(vec![
        label(components, Expression::text("Checkboxes")?),
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
    ]);
    let images = page(vec![
        label(components, Expression::text("Images")?),
        ComponentPart::typed(components.image, |_, _| {
            ImageProps::new("images/pixui-logo.png")
        })
        .with_layout(LayoutStyle::fixed(160.0, 160.0))
        .into(),
    ]);
    let collections = page(vec![
        label(components, Expression::text("Collections")?),
        button(components, "Add collection row", add_sample)?,
        LivePart::ForLoop(ForLoopPart {
            key: None,
            expression: Expression::from_collection(samples),
            body: Box::new(
                ComponentPart::typed(components.label, |context, _| {
                    let sample = context
                        .value()?
                        .downcast_ref::<Sample>()
                        .ok_or_else(|| pixui_error!("sample row requires Sample context"))?;
                    Ok(LabelProps {
                        text: sample.label.clone(),
                    })
                })
                .into(),
            ),
        }),
    ]);
    let text_page = page(vec![
        label(components, Expression::text("Text and translations")?),
        label(
            components,
            Expression::text("Shared data, hover and focus · independent theme and locale")?,
        ),
        label(
            components,
            Expression::i18n("Counter: {count}", [("count", Expression::entity(count))])?,
        ),
        ComponentPart::typed_with_expressions(
            components.button,
            vec![Expression::text("Focus without activation")?],
            |_, _, values| {
                Ok(ButtonProps {
                    label: text(values, 0)?,
                })
            },
        )
        .with_focus(pixui_engine::ui::focus::FocusBehavior::Sequential)
        .into(),
    ]);
    let navigation = ContainerPart::column()
        .with_layout(LayoutStyle {
            width: Length::Pixels(SIDEBAR_WIDTH),
            min_width: Length::Pixels(SIDEBAR_WIDTH),
            max_width: Length::Pixels(SIDEBAR_WIDTH),
            shrink: 0.0,
            ..Default::default()
        })
        .with_gap(8.0)
        .with_children(vec![
            label(components, Expression::text("Components")?),
            navigation_item::<0>(components, "Buttons")?,
            navigation_item::<1>(components, "Checkboxes")?,
            navigation_item::<2>(components, "Images")?,
            navigation_item::<3>(components, "Collections")?,
            navigation_item::<4>(components, "Text and translations")?,
        ])
        .into();
    let selected_page = LivePart::Match(MatchPart::new(
        Expression::entity(selected),
        [buttons, checkboxes, images, collections, text_page]
            .into_iter()
            .enumerate()
            .map(|(index, part)| MatchCandidate {
                pattern: MatchPattern::value(index as u64),
                part,
            })
            .collect(),
    )?);
    let mut panes = ContainerPart::grid()
        .with_layout(LayoutStyle {
            grow: 1.0,
            shrink: 0.0,
            ..Default::default()
        })
        .with_columns(vec![Track::length(SIDEBAR_WIDTH), Track::fraction(1.0)])
        .with_gap(20.0)
        .with_children(vec![navigation, selected_page]);
    panes.align = pixui_engine::layout::style::Alignment::Start;
    let root = ContainerPart::column()
        .with_layout(LayoutStyle {
            grow: 1.0,
            shrink: 0.0,
            ..Default::default()
        })
        .with_gap(20.0)
        .with_children(vec![
            label(components, Expression::text("PixUI · Widget showcase")?),
            panes.into(),
            label(
                components,
                Expression::text("Tab: focus · Enter/Space: activate · F11: performance")?,
            ),
        ])
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

fn page(parts: Vec<LivePart>) -> LivePart {
    ContainerPart::column()
        .with_gap(12.0)
        .with_layout(LayoutStyle {
            width: Length::Percent(1.0),
            min_width: Length::Pixels(0.0),
            ..Default::default()
        })
        .with_children(parts)
        .into()
}

/// Navigation uses ordinary buttons to change the shared selected page.
fn navigation_item<const PAGE: u64>(
    components: StandardComponents,
    title: &str,
) -> PixuiResult<LivePart> {
    Ok(ComponentPart::typed_with_expressions(
        components.button,
        vec![Expression::text(title)?],
        |_, _, values| {
            Ok(ButtonProps {
                label: text(values, 0)?,
            })
        },
    )
    .with_layout(LayoutStyle {
        width: Length::Percent(1.0),
        min_width: Length::Pixels(0.0),
        ..Default::default()
    })
    .with_activation(select_page::<PAGE>)
    .into())
}
fn select_page<const PAGE: u64>(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    let action = context
        .application()?
        .slice_named("showcase")?
        .action_handle_named("select")?;
    Ok(Box::new(move |_| action.call(vec![Box::new(PAGE)])))
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
