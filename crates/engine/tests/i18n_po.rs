use pixui_engine::{
    expression::expression::Expression,
    i18n::{format::TranslationFormat, po::PoFormat, registry::TranslationRegistry},
};

#[test]
fn po_preserves_context_unicode_multiline_and_reports_skipped_translations() {
    let catalog = PoFormat
        .import(include_str!("fixtures/i18n/messages.po"))
        .unwrap();
    assert_eq!(catalog.entries.len(), 2);
    assert!(
        catalog
            .entries
            .iter()
            .any(|entry| entry.key.source == "Open"
                && entry.key.context.as_deref() == Some("action")
                && entry.translation == "Öffnen")
    );
    assert!(
        catalog
            .entries
            .iter()
            .any(|entry| entry.key.source == "Hello {name}\n\"Welcome\""
                && entry.translation == "Hallo {name}\n\"Willkommen\"")
    );
    assert_eq!(catalog.diagnostics.len(), 3);
}

#[test]
fn pot_export_is_deterministic_and_round_trips_metadata_and_escaping() {
    let mut registry = TranslationRegistry::default();
    for source in ["Zebra", "Quotes \"x\"\nBackslash \\", "Álpha"] {
        registry
            .register_expression(
                "one",
                &Expression::text(source)
                    .unwrap()
                    .with_translation_context("context")
                    .with_translator_comment("Line one\nLine two"),
            )
            .unwrap();
    }
    registry
        .register_expression("two", &Expression::text("Excluded").unwrap())
        .unwrap();
    let declarations = registry.declarations("one");
    let exported = PoFormat.export(&declarations).unwrap();
    let mut reversed = declarations.clone();
    reversed.reverse();
    assert_eq!(exported, PoFormat.export(&reversed).unwrap());
    assert!(!exported.contains("Excluded"));
    assert!(!exported.contains("POT-Creation-Date"));
    let parsed = ferrocat_po::parse_po(&exported).unwrap();
    assert_eq!(parsed.items.len(), 3);
    for declaration in declarations {
        let item = parsed
            .items
            .iter()
            .find(|item| item.msgid == declaration.key.source)
            .unwrap();
        assert_eq!(item.msgctxt, declaration.key.context);
        assert!(item.extracted_comments.contains(&"Line two".into()));
        assert!(!item.references.is_empty());
    }
    let empty_translations = PoFormat.import(&exported).unwrap();
    assert!(empty_translations.entries.is_empty());
    assert_eq!(empty_translations.diagnostics.len(), 3);
}

#[test]
fn po_rejects_plural_bad_syntax_duplicate_keys_and_non_utf8_metadata() {
    for input in [
        "msgid \"one\"\nmsgid_plural \"many\"\nmsgstr[0] \"eins\"\nmsgstr[1] \"viele\"\n",
        "msgid \"broken\nmsgstr \"text\"\n",
        "msgid \"same\"\nmsgstr \"eins\"\n\nmsgid \"same\"\nmsgstr \"zwei\"\n",
        "msgid \"\"\nmsgstr \"Content-Type: text/plain; charset=ISO-8859-1\\n\"\n",
    ] {
        assert!(PoFormat.import(input).is_err(), "{input}");
    }
}
