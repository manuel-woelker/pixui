# Internationalization

Declare messages in the UI tree, register them once, and evaluate against an
instance's indexed language. Rendering receives ordinary owned strings; draw
lists and GUI renderers need no translation logic.

## Messages and placeholders

`Expression::text("Save")?` declares a message without arguments.
`Expression::i18n("Counter: {count}", [("count", expression)])?` binds named
subexpressions. Use `{name}`, with ASCII identifier names, and `{{` / `}}` for
literal braces. Translations can reorder or repeat placeholders, but must keep
the same name set. Each argument evaluates once per occurrence of the message.

Supported arguments are owned string types, booleans, integers, and floats.
Numbers use Rust's locale-independent formatting. Collections and arbitrary
objects return errors. Nested messages inherit the selected language and domain.
Templates are limited to 64 KiB, expanded strings to 1 MiB, and expression
nesting to 32 levels.

`Expression::computed(callback)` supplies derived values, such as an open-task
count. It must not construct hidden i18n declarations: registration cannot
inspect arbitrary callback code.

```rust
use pixui_engine::{
    application::app::Application,
    expression::{context::ExpressionContext, evaluator::evaluate, expression::Expression},
    i18n::{format::TranslationFormat, po::PoFormat},
};

let mut app = Application::default();
let expression = app.register_expression("settings", &Expression::text("Save")?)?;
let german = app.register_language("de")?;
let catalog = PoFormat.import("msgid \"Save\"\nmsgstr \"Speichern\"\n")?;
app.install_translations("settings", german, catalog)?;

let context = ExpressionContext::new(&app).with_language(german);
let result = evaluate(&context, &expression)?;
assert_eq!(result.downcast_ref::<String>().unwrap(), "Speichern");
# Ok::<(), pixui_base::PixuiError>(())
```

## Definition declarations

Use `ComponentPart::typed_with_expressions(id, expressions, resolver)`.
The resolver receives context, presentation settings, and an ordered slice of
evaluated `DynamicObject` values. Read these into typed props. Expressions are
owned by the component part, separately from its shared resolver.

Native titles use
`UiDefinition::with_window_property_expressions(expressions, resolver)`.
Existing callback-only component and window APIs remain available, but messages
created inside those callbacks cannot be extracted or automatically registered.

Registration visits the abstract tree, including every match arm, loop body,
nested placeholder, and native-property declaration. It never expands collection
rows or invokes callbacks. It registers private copies and commits only after
the entire definition succeeds.

## Domains and identity

Every definition has one domain, defaulting to its name. Set an explicit stable
domain with `with_translation_domain("settings")`. Messages cannot override
their definition's domain. Multiple definitions can intentionally share a
domain.

Identity is `(domain, source, optional semantic context)`. Identical keys reuse
one index even when they bind different data. Use
`with_translation_context("action")` to distinguish a verb from a status.
`with_translator_comment(...)` adds explanation without changing identity.
Comments and declaration locations from shared occurrences are merged.

Source or domain renames create new catalog keys. Runtime message indices are
append-only and application-local; zero is unresolved. Reusing declarations in
another application resolves fresh indices. Never store indices in catalog
files.

## Languages and updates

Register tags with `register_language("de")` and configure the source tag
(default `en`) with `set_source_language` before adding other languages.
Tag matching is ASCII case insensitive. Exact selection is used: `de-AT` does
not implicitly fall back to `de`.

`PresentationSettings.language` selects translations. Its default zero selects
source text. `locale` remains available for custom formatting and callbacks but
does not select a catalog by itself. Use
`presentation_language(settings, "de")` to set both. Send the resulting settings
through `UiCommand::Present` to switch a live instance.

Language and resolved-message indices carry registry identity. Foreign indices
are rejected; source-language zero is portable. Non-UI expression contexts
default to source language, and loop contexts preserve explicit selection.

Install one domain/language catalog with `install_translations`. Loading before
definition registration works: stable-key catalogs are retained and populate new
slots later. Missing entries fall back directly to source. Replacement clears
missing entries in that domain, preserves other domains, and validates all
entries before committing. The returned report contains nonfatal diagnostics
for skipped or unmatched entries.

Catalog updates currently invalidate all UIs, following existing content
invalidation behavior; focus/hover are cleared. Hidden windows receive native
metadata updates without painting. Old render outputs retain their strings.
Expression errors preserve the last good frame and title independently.

## Catalog adapters and workflow

`TranslationFormat` imports neutral `TranslationCatalog` records and exports
source `MessageDeclaration` records. Pass an adapter explicitly; no per-message
plugin dispatch occurs. The registry compiles templates and uses dense language
vectors, independent of the input file format.

`PoFormat` supports UTF-8 singular PO entries, contexts, multiline strings,
escapes, comments, and deterministic POT export. It uses `ferrocat-po` with
additional quoted-line validation. Fuzzy, obsolete, and empty translations are
skipped with diagnostics. Plural entries and invalid placeholders are errors.
Empty translations mean missing in both the neutral catalog and PO import;
intentional blank translations are deferred.

Keep a separate file per domain/language, such as
`translations/settings/de.po`. The caller supplies the domain when installing;
`msgctxt` remains the semantic context. Export with
`ApplicationHandle::export_translations(domain, &PoFormat)`.
Runtime indices are never exported.

The showcase provides a display-free extraction command:

```bash
./t cargo run -p pixui-example-showcase -- --export-translations /tmp/showcase.pot
```

Translate a copy of the POT with a PO editor or Weblate, preserving named
placeholders. Import the completed PO and install it for the selected domain and
language. Gettext's built-in checks do not validate our custom brace grammar;
the engine validates it during installation.

Plurals, grammatical selectors, localized dates/numbers, rich text, and general
bidi/shaping are outside this implementation. A richer format such as Fluent
would need a richer evaluator for those features, not only another adapter. See
[the decision record](../../../../docs/decisions/DR-014%20Register%20declared%20translations%20by%20definition%20domain.md).
