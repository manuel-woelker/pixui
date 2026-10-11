use super::parse::parse;
use crate::ui_language::{
    ast::{
        expression::{ExpressionKind, SegmentKind},
        part::PartKind,
    },
    diagnostic::{render::render, report::Stage},
    source::snapshot::Source,
};

#[test]
fn parses_bare_attributes_templates_nested_controls_and_fragments() {
    let source = Source::new(
        "example.pixui",
        r#"ui Example domain 'demo' {
        <Column gap=8>
            <Label text="Hello {todo.title}" />
            {for todo in collections.todos => <>
                <Checkbox checked=!todo.done enabled=entities.enabled && !todo.locked />
                {match todo.done; true => <Label text='Done {{ok}}' />, false => <Label text="Open" />,}
            </>}
        </Column>
    }"#,
    );
    let definition = parse(&source).unwrap();
    assert_eq!(source.slice(definition.name.span), Some("Example"));
    let PartKind::Element { children, .. } = definition.root.kind else {
        panic!()
    };
    let PartKind::Element { attributes, .. } = &children[0].kind else {
        panic!()
    };
    let ExpressionKind::Template {
        translated,
        segments,
    } = &attributes[0].value.kind
    else {
        panic!()
    };
    assert!(*translated);
    assert!(matches!(segments[1].kind, SegmentKind::Value(_)));
    assert_eq!(source.slice(segments[1].span), Some("{todo.title}"));
    assert!(matches!(children[1].kind, PartKind::For { .. }));
}
#[test]
fn strings_do_not_confuse_markup_or_interpolation_delimiters() {
    let source = Source::new(
        "quoted.pixui",
        r#"ui X {<Label text='<> /> {"inner {entity.name}"} {{x}}' />}"#,
    );
    assert!(parse(&source).is_ok());
}
#[test]
fn diagnostics_have_snapshot_locations_and_secondary_labels() {
    let source = Source::new("bad.pixui", "ui X {\r\n<Column>\r\n</Row>}");
    let errors = parse(&source).unwrap_err();
    assert_eq!(errors[0].stage, Stage::Parser);
    assert_eq!(source.slice(errors[0].primary), Some("Row"));
    assert_eq!(errors[0].secondary.len(), 1);
    assert!(render(&source, &errors[0]).contains("bad.pixui:3:3"));
    let new_source = Source::new("bad.pixui", "changed");
    assert!(new_source.slice(errors[0].primary).is_none());
}
#[test]
fn invalid_inputs_are_bounded_and_source_located() {
    for input in [
        "",
        "ui X {<Label text='unterminated />}",
        "ui X {<Label text='\\z' />}",
        "ui X {<Label text=1enabled=true />}",
        "ui X {<Label text=@ />}",
        "ui X { {match true true => <Label />,} }",
        "ui X { <Label text='}' /> }",
    ] {
        let source = Source::new("invalid.pixui", input);
        let errors = parse(&source).unwrap_err();
        assert!(!errors.is_empty() && errors.len() <= 33);
        for error in errors {
            assert!(source.slice(error.primary).is_some());
        }
    }
}
#[test]
fn unicode_columns_and_eof_spans_are_checked() {
    let source = Source::new("utf.pixui", "é\r\n😀");
    assert!(source.span(1, 2).is_none());
    assert_eq!(source.position(4), Some((2, 1)));
    assert_eq!(source.position(8), Some((2, 2)));
    assert_eq!(source.slice(source.span(8, 8).unwrap()), Some(""));
}

#[test]
fn nesting_input_size_and_recovery_limits_are_diagnostics() {
    let deep = format!("ui X {{<Label text={}true />}}", "!".repeat(200));
    let errors = parse(&Source::new("deep.pixui", deep)).unwrap_err();
    assert!(errors.iter().any(|error| error.message.contains("nesting")));
    let large = " ".repeat(crate::ui_language::lexer::scan::MAX_SOURCE_BYTES + 1);
    let errors = parse(&Source::new("large.pixui", large)).unwrap_err();
    assert_eq!(errors[0].code, "L001");
    let source = Source::new("many.pixui", format!("ui X {{<>{}</>}}", "?".repeat(200)));
    assert_eq!(parse(&source).unwrap_err().len(), 32);
}
#[test]
fn character_deletions_do_not_panic_or_produce_invalid_spans() {
    let text = r#"ui X {<Column><Label text="é {entities.name}" />{match true; true => <Label text='a' />, false => <Label text='b' />,}</Column>}"#;
    for (offset, ch) in text.char_indices() {
        let mut input = text.to_owned();
        input.replace_range(offset..offset + ch.len_utf8(), "");
        let source = Source::new("mutated.pixui", input);
        if let Err(errors) = parse(&source) {
            assert!(errors.len() <= 32);
            for error in errors {
                assert!(source.slice(error.primary).is_some());
            }
        }
    }
}
