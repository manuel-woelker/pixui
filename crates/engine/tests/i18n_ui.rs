//! Real worker tests for registration, translation updates, and retained outputs.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::{
        action::slice_actions, app::Application, application_handle::ApplicationHandle,
        application_slice::ApplicationSlice, entity_mut::EntityMut,
    },
    components::label::LabelProps,
    expression::{context::ExpressionContext, expression::Expression},
    i18n::{
        catalog::{MessageKey, TranslationCatalog, TranslationEntry},
        format::TranslationFormat,
        po::PoFormat,
    },
    live_model::{
        match_part::{MatchCandidate, MatchPart, MatchPattern},
        part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
    },
    ui::{
        definition::UiDefinition,
        display_list::{DrawCommand, RenderOutput},
        input::UiCommand,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
        window_properties::{WindowCommand, WindowProperties},
    },
};
use pixui_reflect::{DynamicObject, Reflect};
use std::time::Duration;

#[slice_actions(slice = "test", facade = TestActions)]
mod actions {
    use super::*;
    #[action]
    pub fn increment(mut count: EntityMut<u64>) {
        *count += 1;
    }
    #[action]
    pub fn toggle_failure(mut broken: EntityMut<bool>) {
        *broken = !*broken;
    }
}
fn setup() -> ApplicationHandle {
    let app = Application::new();
    let mut slice = ApplicationSlice::new("test");
    slice.bind("count", 7_u64).unwrap();
    slice.bind("broken", false).unwrap();
    let slice = app.add_slice(slice).unwrap();
    actions::TestActions::register(&app, slice).unwrap();
    app
}
fn components(app: &ApplicationHandle) -> pixui_engine::painters::standard::StandardComponents {
    let components = app.register_standard_components().unwrap();
    app.register_standard_painters().unwrap();
    components
}
fn value<'a>(context: &ExpressionContext<'a>) -> PixuiResult<DynamicObject<'a>> {
    let app = context.application()?;
    let slice = app.slice_named("test")?.id();
    if *app.entity::<bool>(slice, "broken")? {
        return Err(pixui_error!("broken argument"));
    }
    Ok(DynamicObject::from_reflect(
        *app.entity::<u64>(slice, "count")?,
    ))
}
fn expression() -> Expression {
    Expression::i18n("Value: {n}", [("n", Expression::computed(value))]).unwrap()
}
fn definition(app: &ApplicationHandle, name: &str) -> UiDefinition {
    let label = components(app).label;
    UiDefinition::new(
        name,
        LivePart::Component(ComponentPart::typed_with_expressions(
            label,
            vec![expression()],
            |_, _, values| {
                Ok(LabelProps {
                    text: values[0].downcast_ref::<String>().unwrap().clone(),
                })
            },
        )),
    )
    .with_translation_domain("test")
    .with_window_property_expressions(vec![expression()], |_, _, values| {
        Ok(WindowProperties {
            title: values[0].downcast_ref::<String>().unwrap().clone().into(),
            icon: None,
        })
    })
}
fn receive(outputs: &OutputReceiver) -> RenderOutput {
    outputs.recv_timeout(Duration::from_secs(2)).unwrap()
}
fn translated(output: &RenderOutput, expected: &str) -> bool {
    output
        .display_list
        .commands
        .iter()
        .any(|command| matches!(command, DrawCommand::DrawText { text, .. } if text == expected))
}
fn catalog(text: &str) -> TranslationCatalog {
    TranslationCatalog {
        entries: vec![TranslationEntry {
            key: MessageKey {
                source: "Value: {n}".into(),
                context: None,
            },
            translation: text.into(),
        }],
        ..Default::default()
    }
}

#[test]
fn language_switch_catalog_replacement_and_hidden_metadata_keep_outputs_valid() {
    let app = setup();
    let definition = app.register_ui(definition(&app, "test")).unwrap();
    let de = app.register_language("de").unwrap();
    app.install_translations("test", de, catalog("Wert: {n}"))
        .unwrap();
    let (one, outputs) = app
        .create_ui(
            definition,
            PresentationSettings {
                locale: "de".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let (two, other) = app
        .create_ui(
            definition,
            app.presentation_language(PresentationSettings::default(), "de")
                .unwrap(),
        )
        .unwrap();
    let source = receive(&outputs);
    let german = receive(&other);
    assert!(
        translated(&source, "Value: 7"),
        "locale alone must not select catalog"
    );
    assert!(translated(&german, "Wert: 7"));
    app.inspect(|_| Ok(())).unwrap();
    while outputs.window_commands().try_recv().is_ok() {}
    app.ui_command(UiCommand::Present {
        instance: one,
        settings: app
            .presentation_language(PresentationSettings::default(), "de")
            .unwrap(),
    })
    .unwrap();
    assert!(translated(&receive(&outputs), "Wert: 7"));
    app.ui_command(UiCommand::Visibility {
        instance: one,
        visible: false,
    })
    .unwrap();
    app.install_translations("test", de, catalog("{n}: neuer Wert"))
        .unwrap();
    app.inspect(|_| Ok(())).unwrap();
    assert!(outputs.try_recv().is_err());
    assert_eq!(
        outputs.window_commands().try_recv().unwrap(),
        WindowCommand::SetTitle("7: neuer Wert".into())
    );
    assert!(translated(&receive(&other), "7: neuer Wert"));
    assert!(
        translated(&german, "Wert: 7"),
        "retained output is immutable"
    );
    let foreign = Application::new().register_language("de").unwrap();
    assert!(
        app.create_ui(
            definition,
            PresentationSettings {
                language: foreign,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        app.ui_command(UiCommand::Present {
            instance: two,
            settings: PresentationSettings {
                language: foreign,
                ..Default::default()
            }
        })
        .is_err()
    );
    assert!(
        app.install_translations("test", de, catalog("{bad}"))
            .is_err()
    );
    app.ui_command(UiCommand::Visibility {
        instance: one,
        visible: true,
    })
    .unwrap();
    assert!(translated(&receive(&outputs), "7: neuer Wert"));
    actions::TestActions::bind(&app)
        .unwrap()
        .increment()
        .unwrap();
    assert!(translated(&receive(&outputs), "8: neuer Wert"));
    assert!(translated(&receive(&other), "8: neuer Wert"));
}

#[test]
fn evaluation_failure_preserves_last_good_frame_and_title_then_recovers() {
    let app = setup();
    let definition = app.register_ui(definition(&app, "test")).unwrap();
    let (instance, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let initial = receive(&outputs);
    app.inspect(|_| Ok(())).unwrap();
    while outputs.window_commands().try_recv().is_ok() {}
    let actions = actions::TestActions::bind(&app).unwrap();
    actions.toggle_failure().unwrap();
    app.inspect(move |app| {
        let instance = app.uis().instance(instance)?;
        assert_eq!(instance.revision(), initial.revision);
        assert!(instance.last_error().unwrap().contains("broken argument"));
        assert!(
            instance
                .window_properties_error()
                .unwrap()
                .contains("broken argument")
        );
        Ok(())
    })
    .unwrap();
    assert!(outputs.try_recv().is_err());
    assert!(outputs.window_commands().try_recv().is_err());
    actions.toggle_failure().unwrap();
    assert!(translated(&receive(&outputs), "Value: 7"));
}

#[pixui_reflect::reflect]
mod model {
    pub struct Row {
        pub name: String,
    }
}

#[test]
fn extraction_visits_hidden_arms_empty_loops_and_nested_titles_and_rolls_back_failed_ui() {
    use pixui_engine::application::collection::Collection;
    let app = setup();
    let components = components(&app);
    let slice = app
        .inspect(|app| Ok(app.slice_named("test")?.id()))
        .unwrap();
    app.add_collection(slice, Collection::new_reflected::<model::Row>("rows"))
        .unwrap();
    let rows = app.collection_key("test", "rows").unwrap();
    let broken = app.entity_ref::<bool>(slice, "broken").unwrap();
    let label = |message| {
        LivePart::Component(ComponentPart::typed_with_expressions(
            components.label,
            vec![message],
            |_, _, values| {
                Ok(LabelProps {
                    text: values[0].downcast_ref::<String>().unwrap().clone(),
                })
            },
        ))
    };
    let row = Expression::i18n(
        "Row {name}",
        [(
            "name",
            Expression::field(model::Row::type_descriptor().field_index("name").unwrap()),
        )],
    )
    .unwrap();
    let tree = LivePart::Composite(CompositePart {
        parts: vec![
            LivePart::Match(
                MatchPart::new(
                    Expression::entity(broken),
                    vec![MatchCandidate {
                        pattern: MatchPattern::value(true),
                        part: label(Expression::text("Hidden").unwrap()),
                    }],
                )
                .unwrap(),
            ),
            LivePart::ForLoop(ForLoopPart {
                key: None,
                expression: Expression::from_collection(rows),
                body: Box::new(label(row)),
            }),
        ],
    });
    let definition = UiDefinition::new("extract", tree)
        .with_translation_domain("domain")
        .with_window_property_expressions(
            vec![
                Expression::i18n(
                    "Title {inner}",
                    [(
                        "inner",
                        Expression::text("Nested")
                            .unwrap()
                            .with_translator_comment("Title word"),
                    )],
                )
                .unwrap(),
            ],
            |_, _, values| {
                Ok(WindowProperties {
                    title: values[0].downcast_ref::<String>().unwrap().clone().into(),
                    icon: None,
                })
            },
        );
    app.register_ui(definition).unwrap();
    let exported = app.export_translations("domain", &PoFormat).unwrap();
    for text in [
        "Hidden",
        "Row {name}",
        "Title {inner}",
        "Nested",
        "Title word",
    ] {
        assert!(exported.contains(text), "{text}");
    }
    let before = app.translation_messages("domain").unwrap();
    assert_eq!(before.len(), 4);
    assert!(
        app.register_ui(
            UiDefinition::new(
                "extract",
                label(Expression::text("Must roll back").unwrap())
            )
            .with_translation_domain("domain")
        )
        .is_err()
    );
    assert_eq!(app.translation_messages("domain").unwrap(), before);
    assert_eq!(exported, PoFormat.export(&before).unwrap());
}

#[test]
fn loop_fields_retain_language_and_empty_catalog_falls_back() {
    use pixui_engine::application::collection::Collection;
    let app = setup();
    let slice = app
        .inspect(|app| Ok(app.slice_named("test")?.id()))
        .unwrap();
    let mut rows = Collection::new_reflected::<model::Row>("rows");
    rows.arena_mut::<model::Row>()
        .unwrap()
        .insert(model::Row { name: "Ada".into() });
    app.add_collection(slice, rows).unwrap();
    let label = components(&app).label;
    let row = Expression::i18n(
        "Row {name}",
        [(
            "name",
            Expression::field(model::Row::type_descriptor().field_index("name").unwrap()),
        )],
    )
    .unwrap();
    let definition = app
        .register_ui(UiDefinition::new(
            "rows",
            LivePart::ForLoop(ForLoopPart {
                key: None,
                expression: Expression::from_collection(
                    app.collection_key("test", "rows").unwrap(),
                ),
                body: Box::new(LivePart::Component(ComponentPart::typed_with_expressions(
                    label,
                    vec![row],
                    |_, _, values| {
                        Ok(LabelProps {
                            text: values[0].downcast_ref::<String>().unwrap().clone(),
                        })
                    },
                ))),
            }),
        ))
        .unwrap();
    let de = app.register_language("de").unwrap();
    app.install_translations(
        "rows",
        de,
        TranslationCatalog {
            entries: vec![TranslationEntry {
                key: MessageKey {
                    source: "Row {name}".into(),
                    context: None,
                },
                translation: "{name}: Zeile".into(),
            }],
            ..Default::default()
        },
    )
    .unwrap();
    let (_, outputs) = app
        .create_ui(
            definition,
            app.presentation_language(PresentationSettings::default(), "de")
                .unwrap(),
        )
        .unwrap();
    assert!(translated(&receive(&outputs), "Ada: Zeile"));
    app.install_translations("rows", de, TranslationCatalog::default())
        .unwrap();
    assert!(translated(&receive(&outputs), "Row Ada"));
}
