//! Registration and evaluation contracts independent of native rendering.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::{app::Application, application_slice::ApplicationSlice},
    expression::{
        context::ExpressionContext,
        evaluator::evaluate,
        expression::{Expression, ExpressionKind},
    },
    i18n::{
        catalog::{MessageKey, TranslationCatalog, TranslationEntry},
        registry::TranslationRegistry,
        template::{CompiledTemplate, MAX_TEMPLATE_BYTES},
    },
};
use pixui_reflect::DynamicObject;

fn index(expression: &Expression) -> usize {
    let ExpressionKind::I18n(message) = expression.kind() else {
        panic!("message")
    };
    message.index().get()
}
fn catalog(source: &str, translation: &str) -> TranslationCatalog {
    TranslationCatalog {
        entries: vec![TranslationEntry {
            key: MessageKey {
                source: source.into(),
                context: None,
            },
            translation: translation.into(),
        }],
        ..Default::default()
    }
}

#[test]
fn registry_deduplicates_within_domains_and_rebinds_foreign_expressions() {
    let message = Expression::text("Open")
        .unwrap()
        .with_translator_comment("A button");
    assert_eq!(index(&message), 0);
    let mut one = TranslationRegistry::default();
    let a = one.register_expression("files", &message).unwrap();
    let b = one.register_expression("files", &message).unwrap();
    let c = one.register_expression("status", &message).unwrap();
    let d = one
        .register_expression("files", &message.clone().with_translation_context("status"))
        .unwrap();
    assert_eq!(index(&a), index(&b));
    assert_ne!(index(&a), index(&c));
    assert_ne!(index(&a), index(&d));
    let mut two = TranslationRegistry::default();
    two.register_expression("other", &Expression::text("Other").unwrap())
        .unwrap();
    let rebound = two.register_expression("files", &a).unwrap();
    assert_eq!(index(&rebound), 2);
    assert_eq!(two.declarations("files").len(), 1);
    assert_eq!(one.declarations("files").len(), 2);
    assert_eq!(one.declarations("files")[0].comments.len(), 1);
}

#[test]
fn catalogs_loaded_before_definitions_support_partial_fallback_and_atomic_replacement() {
    let mut registry = TranslationRegistry::default();
    let de = registry.register_language("DE").unwrap();
    assert_eq!(de, registry.register_language("de").unwrap());
    let report = registry
        .install("one", de, catalog("Hello", "Hallo"))
        .unwrap();
    assert_eq!(report.diagnostics.len(), 1);
    let hello = registry
        .register_expression("one", &Expression::text("Hello").unwrap())
        .unwrap();
    let other = registry
        .register_expression("two", &Expression::text("Hello").unwrap())
        .unwrap();
    registry
        .install("two", de, catalog("Hello", "Guten Tag"))
        .unwrap();
    assert_eq!(index(&hello), 1);
    assert_eq!(index(&other), 2);
    assert!(
        registry
            .install("one", de, catalog("Hello", "{unexpected}"))
            .is_err()
    );
    let foreign = TranslationRegistry::default()
        .register_language("de")
        .unwrap();
    assert!(registry.validate_language(foreign).is_err());
    assert!(registry.language("fr").is_err());
    assert!(
        registry
            .install("one", Default::default(), catalog("Hello", "Hi"))
            .is_err()
    );
    registry
        .install("one", de, TranslationCatalog::default())
        .unwrap();
    assert_eq!(registry.declarations("two").len(), 1);
}

#[test]
fn interpolation_reads_entities_fields_and_nested_messages_in_selected_language() {
    let mut app = Application::default();
    let mut slice = ApplicationSlice::new("test");
    slice.bind("count", 3_u64).unwrap();
    slice.bind("name", String::from("Ada")).unwrap();
    let slice = app.add_slice(slice).unwrap();
    let count = Expression::entity(app.entity_ref::<u64>(slice, "count").unwrap());
    let name = Expression::entity(app.entity_ref::<String>(slice, "name").unwrap());
    let message = Expression::i18n(
        "Hello {name}: {count} / {count} {{ok}}",
        [("name", name), ("count", count)],
    )
    .unwrap();
    let registered = app.register_expression("test", &message).unwrap();
    let de = app.register_language("de").unwrap();
    app.install_translations(
        "test",
        de,
        catalog(
            "Hello {name}: {count} / {count} {{ok}}",
            "{count} / {count}: Hallo {name} {{gut}}",
        ),
    )
    .unwrap();
    let context = ExpressionContext::new(&app).with_language(de);
    assert_eq!(
        evaluate(&context, &registered)
            .unwrap()
            .downcast_ref::<String>()
            .unwrap(),
        "3 / 3: Hallo Ada {gut}"
    );
    assert_eq!(
        evaluate(&ExpressionContext::new(&app), &registered)
            .unwrap()
            .downcast_ref::<String>()
            .unwrap(),
        "Hello Ada: 3 / 3 {ok}"
    );
    let nested = Expression::i18n("Nested: {inner}", [("inner", registered.clone())]).unwrap();
    let nested = app.register_expression("test", &nested).unwrap();
    assert_eq!(
        evaluate(&ExpressionContext::new(&app).with_language(de), &nested)
            .unwrap()
            .downcast_ref::<String>()
            .unwrap(),
        "Nested: 3 / 3: Hallo Ada {gut}"
    );
    let foreign = TranslationRegistry::default()
        .register_expression("test", &message)
        .unwrap();
    assert!(evaluate(&ExpressionContext::new(&app), &foreign).is_err());
    let raw = Expression::text("Unresolved").unwrap();
    assert!(evaluate(&ExpressionContext::new(&app), &raw).is_err());
    assert!(
        evaluate(
            &ExpressionContext::from_value(&DynamicObject::from_reflect(1_u64)),
            &raw
        )
        .is_err()
    );
}

#[test]
fn templates_reject_malformed_names_and_bound_input() {
    for source in ["{", "}", "{9name}", "{}", "{a b}", "{a.b}", "{é}"] {
        assert!(CompiledTemplate::parse(source).is_err(), "{source}");
    }
    assert!(CompiledTemplate::parse("{{hello}} {name} {name}").is_ok());
    assert!(CompiledTemplate::parse(&"x".repeat(MAX_TEMPLATE_BYTES + 1)).is_err());
    assert!(Expression::i18n("{name}", [("other", Expression::text("x").unwrap())]).is_err());
    assert!(
        Expression::i18n(
            "{name}",
            [
                ("name", Expression::text("x").unwrap()),
                ("name", Expression::text("x").unwrap())
            ]
        )
        .is_err()
    );
    let mut registry = TranslationRegistry::default();
    assert!(
        registry
            .register_expression("", &Expression::text("x").unwrap())
            .is_err()
    );
    assert!(registry.register_language("en--US").is_err());
}

#[test]
fn nested_registration_is_bounded_and_rolls_back() -> PixuiResult<()> {
    let mut expression = Expression::text("Leaf")?;
    for _ in 0..40 {
        expression = Expression::i18n("{nested}", [("nested", expression)])?;
    }
    let mut registry = TranslationRegistry::default();
    assert!(registry.register_expression("test", &expression).is_err());
    assert!(registry.declarations("test").is_empty());
    let next = registry.register_expression("test", &Expression::text("Next")?)?;
    assert_eq!(index(&next), 1);
    Ok(())
}

#[test]
fn evaluation_uses_dense_catalogs_after_late_registration_and_domain_replacement() {
    let mut app = Application::default();
    let de = app.register_language("de").unwrap();
    app.install_translations("one", de, catalog("Hello", "Hallo"))
        .unwrap();
    app.install_translations("two", de, catalog("Hello", "Guten Tag"))
        .unwrap();
    let a = app
        .register_expression("one", &Expression::text("Hello").unwrap())
        .unwrap();
    let b = app
        .register_expression("two", &Expression::text("Hello").unwrap())
        .unwrap();
    let text = |app: &Application, expression: &Expression| {
        evaluate(&ExpressionContext::new(app).with_language(de), expression)
            .unwrap()
            .downcast_ref::<String>()
            .unwrap()
            .clone()
    };
    assert_eq!(text(&app, &a), "Hallo");
    assert_eq!(text(&app, &b), "Guten Tag");
    assert!(
        app.install_translations("one", de, catalog("Hello", "{bad}"))
            .is_err()
    );
    assert_eq!(text(&app, &a), "Hallo");
    app.install_translations("one", de, TranslationCatalog::default())
        .unwrap();
    assert_eq!(text(&app, &a), "Hello");
    assert_eq!(text(&app, &b), "Guten Tag");
    let missing = app
        .register_expression("two", &Expression::text("Changed source").unwrap())
        .unwrap();
    assert_eq!(text(&app, &missing), "Changed source");
}

#[test]
fn repeated_arguments_evaluate_once_and_scalar_output_is_bounded() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn argument<'a>(_: &ExpressionContext<'a>) -> PixuiResult<DynamicObject<'a>> {
        CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(DynamicObject::from_reflect(42_i64))
    }
    let mut app = Application::default();
    let expr = Expression::i18n("{n}/{n}/{n}", [("n", Expression::computed(argument))]).unwrap();
    let expr = app.register_expression("test", &expr).unwrap();
    assert_eq!(
        evaluate(&ExpressionContext::new(&app), &expr)
            .unwrap()
            .downcast_ref::<String>()
            .unwrap(),
        "42/42/42"
    );
    assert_eq!(CALLS.load(Ordering::Relaxed), 1);
    fn large<'a>(_: &ExpressionContext<'a>) -> PixuiResult<DynamicObject<'a>> {
        Ok(DynamicObject::from_reflect("x".repeat(600_000)))
    }
    let expr = Expression::i18n("{n}{n}", [("n", Expression::computed(large))]).unwrap();
    let expr = app.register_expression("test", &expr).unwrap();
    assert!(evaluate(&ExpressionContext::new(&app), &expr).is_err());
    let combined = Expression::i18n(
        "{a}{b}",
        [
            ("a", Expression::computed(large)),
            ("b", Expression::computed(large)),
        ],
    )
    .unwrap();
    let combined = app.register_expression("test", &combined).unwrap();
    assert!(evaluate(&ExpressionContext::new(&app), &combined).is_err());
    fn object<'a>(_: &ExpressionContext<'a>) -> PixuiResult<DynamicObject<'a>> {
        Ok(DynamicObject::from_reflect(vec![1_u64, 2]))
    }
    let expr = Expression::i18n("{n}", [("n", Expression::computed(object))]).unwrap();
    let expr = app.register_expression("test", &expr).unwrap();
    assert!(evaluate(&ExpressionContext::new(&app), &expr).is_err());
}

#[test]
fn identical_templates_bind_different_values_without_duplicate_indices() {
    let mut app = Application::default();
    let mut slice = ApplicationSlice::new("test");
    slice.bind("one", 1_u64).unwrap();
    slice.bind("two", 2_u64).unwrap();
    let slice = app.add_slice(slice).unwrap();
    let make = |name| {
        Expression::i18n(
            "Count: {n}",
            [(
                "n",
                Expression::entity(app.entity_ref::<u64>(slice, name).unwrap()),
            )],
        )
        .unwrap()
    };
    let one = make("one");
    let two = make("two");
    let one = app.register_expression("test", &one).unwrap();
    let two = app.register_expression("test", &two).unwrap();
    assert_eq!(index(&one), index(&two));
    assert_eq!(app.translations().declarations("test").len(), 1);
    assert_eq!(
        evaluate(&ExpressionContext::new(&app), &one)
            .unwrap()
            .downcast_ref::<String>()
            .unwrap(),
        "Count: 1"
    );
    assert_eq!(
        evaluate(&ExpressionContext::new(&app), &two)
            .unwrap()
            .downcast_ref::<String>()
            .unwrap(),
        "Count: 2"
    );
}
