//! A live component gallery: props read shared data, activations dispatch actions.
use crate::model::data::Sample;
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    components::{
        button::ButtonProps, checkbox::CheckboxProps, image::ImageProps, label::LabelProps,
    },
    expression::{context::ExpressionContext, expression::Expression},
    live_model::{
        match_part::{MatchCandidate, MatchPart, MatchPattern},
        part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
    },
    painters::standard::StandardComponents,
    resources::path::ResourcePath,
    ui::{
        activation::ActionBinding, definition::UiDefinition, presentation::PresentationSettings,
        window_properties::WindowProperties,
    },
};

fn localized<'a>(settings: &PresentationSettings, english: &'a str, german: &'a str) -> &'a str {
    if settings.locale == "de" {
        german
    } else {
        english
    }
}

fn count(context: &ExpressionContext<'_>) -> PixuiResult<u64> {
    let app = context.application()?;
    Ok(*app.entity::<u64>(app.slice_named("showcase")?.id(), "count")?)
}

fn flag(context: &ExpressionContext<'_>, name: &str) -> PixuiResult<bool> {
    let app = context.application()?;
    Ok(*app.entity::<bool>(app.slice_named("showcase")?.id(), name)?)
}

/// Build one definition reused by windows with different themes and locales.
/// Components use standard painters; layout currently stacks constant-height rows.
pub fn definition(
    app: &ApplicationHandle,
    components: StandardComponents,
) -> PixuiResult<UiDefinition> {
    let slice = app.inspect(|app| Ok(app.slice_named("showcase")?.id()))?;
    let details = app.entity_ref::<bool>(slice, "details")?;
    let samples = app.collection_key("showcase", "samples")?;
    let parts = vec![
        LivePart::Component(ComponentPart::typed(components.label, |_, settings| {
            Ok(LabelProps {
                text: localized(settings, "PixUI · Widget showcase", "PixUI · Komponenten").into(),
            })
        })),
        LivePart::Component(ComponentPart::typed(
            components.label,
            |context, settings| {
                Ok(LabelProps {
                    text: format!(
                        "{}: {}",
                        localized(settings, "Counter", "Zähler"),
                        count(context)?
                    ),
                })
            },
        )),
        LivePart::Component(
            ComponentPart::typed(components.button, |_, settings| {
                Ok(ButtonProps {
                    label: localized(settings, "Increment counter", "Zähler erhöhen").into(),
                })
            })
            .with_activation(increment),
        ),
        LivePart::Component(
            ComponentPart::typed(components.button, |_, settings| {
                Ok(ButtonProps {
                    label: localized(settings, "Reset counter", "Zähler zurücksetzen").into(),
                })
            })
            .with_activation(reset),
        ),
        LivePart::Component(
            ComponentPart::typed(components.checkbox, |context, settings| {
                Ok(CheckboxProps {
                    label: localized(
                        settings,
                        "An independent checkbox",
                        "Ein unabhängiges Kontrollkästchen",
                    )
                    .into(),
                    checked: flag(context, "checked")?,
                })
            })
            .with_activation(toggle_checked),
        ),
        LivePart::Component(
            ComponentPart::typed(components.checkbox, |context, settings| {
                Ok(CheckboxProps {
                    label: localized(settings, "Show details", "Details anzeigen").into(),
                    checked: flag(context, "details")?,
                })
            })
            .with_activation(toggle_details),
        ),
        LivePart::Match(MatchPart::new(
            Expression::entity(details),
            vec![MatchCandidate {
                pattern: MatchPattern::value(true),
                part: LivePart::Component(ComponentPart::typed(components.label, |_, settings| {
                    Ok(LabelProps {
                        text: localized(
                            settings,
                            "Shared data, hover and focus · independent theme and locale",
                            "Gemeinsame Daten, Hover und Fokus · eigenes Design und Sprache",
                        )
                        .into(),
                    })
                })),
            }],
        )?),
        LivePart::Component(ComponentPart::typed(components.image, |_, _| {
            ImageProps::new("images/pixui-logo.png")
        })),
        LivePart::Component(
            ComponentPart::typed(components.button, |_, settings| {
                Ok(ButtonProps {
                    label: localized(settings, "Add collection row", "Zeile hinzufügen").into(),
                })
            })
            .with_activation(add_sample),
        ),
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
        LivePart::Component(ComponentPart::typed(components.label, |_, settings| {
            Ok(LabelProps {
                text: localized(
                    settings,
                    "Tab: focus · Enter/Space: activate · F11: performance",
                    "Tab: Fokus · Enter/Leertaste: aktivieren · F11: Leistung",
                )
                .into(),
            })
        })),
    ];
    Ok(
        UiDefinition::new("showcase", LivePart::Composite(CompositePart { parts }))
            .with_window_properties(window_properties),
    )
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
    context: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<WindowProperties> {
    Ok(WindowProperties {
        title: format!(
            "PixUI {} — {}",
            localized(settings, "Showcase", "Komponenten"),
            count(context)?
        )
        .into(),
        icon: Some(ResourcePath::new("images/pixui-logo.png")?),
    })
}
