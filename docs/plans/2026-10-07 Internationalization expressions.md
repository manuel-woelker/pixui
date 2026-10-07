# Internationalization expressions

Status: proposed.

## Goal

Declare translatable messages as expressions in UI definitions. Collect them
before rendering, resolve them to application-wide indices, and evaluate them
using each UI instance's selected language. Export a translator-friendly catalog
without opening windows. Keep file formats separate from expression evaluation.

## Proposed decisions

- Add an i18n expression containing a source template, named placeholder
  subexpressions, optional translation context and translator comment, and a
  translation index initialized to zero.
- Zero means unresolved for message indices. Registration assigns indices
  starting at one; slot zero in every message vector is reserved.
- Each application owns one message registry and a language table. Each language
  owns a dense `Vec<Option<CompiledTemplate>>` indexed by message index.
- Each presentation selects a typed `LanguageIndex`. Language slot zero means
  source language; this is a separate type and namespace from message indices.
- Missing translations fall back to the source template with the same arguments.
  Never send an untranslated placeholder expression to the renderer.
- Register messages when registering definitions, normally during startup.
  Also support later definitions by appending entries without renumbering.
- Each UI definition has one translation domain. All of its messages, including
  nested arguments and window properties, inherit that domain. No expression-
  or subtree-level domain overrides in the initial implementation.
- Export stable domain/source/context keys, never runtime numeric indices.
- Implement named interpolation first. Plural rules, gender selectors, localized
  numeric/date formatting, bidi, and text shaping are separate future work.
- Recommend GNU gettext PO/POT as the first catalog adapter. Make catalog import
  and export replaceable without putting file-format logic in the evaluator.

These are proposed choices for review, not implemented capabilities.

## Current integration gaps

`ExpressionKind` currently supports fields, collections, and named entities. The
evaluator returns a `DynamicObject`. `ExpressionContext` carries application and
loop-value access, but no language. `PresentationSettings.locale` is a string.

Most visible text currently lives in ordinary props-resolver functions. Those
functions are opaque to registration: walking the live tree cannot discover
expressions constructed inside their bodies. A declaration API is necessary;
adding an enum variant alone would leave extraction incomplete.

Native title resolution has the same issue. Source messages used in window
properties must also be declared, not hidden inside its resolver function.

## Expression and declaration API

Illustrative API; exact signatures can change during implementation:

```rust,ignore
let message = Expression::i18n(
    "Counter: {count}",
    [("count", Expression::entity(counter_ref))],
)?; // translation_index == 0

let counter = ComponentPart::typed_with_expressions(
    components.label,
    vec![message],
    |context, settings, values| {
        Ok(LabelProps {
            text: values[0].downcast_ref::<String>()?.clone(),
        })
    },
);
```

- Retain existing function-based props APIs. Add an explicit expression-backed
  binding that owns its expression list and passes evaluated values to a typed
  resolver. Do not introduce a general props language or field-assignment
  system.
- The erased binding must expose declaration traversal and produce a registered
  binding with resolved expression copies. Existing bindings are shared through
  `Arc`; do not mutate a binding shared by multiple definitions/applications.
- Add the corresponding expression-backed window-properties resolver path.
  Both paths use the same evaluator and declaration traversal.
- Registration traverses composite children, all match arms, loop bodies, match
  and loop expressions, binding expressions, declared window-property
  expressions, and nested placeholder expressions. It never executes props
  callbacks, expands collections, or selects only currently visible branches.
- Reuse identical messages with different argument expressions. Deduplicate by
  domain, source, and context, not the identity of an entity or loop item.
  Validate placeholder schemas, merge comments deterministically, and retain
  declaration locations.
- Treat domain/source/context as the portable catalog key. Conflicting
  declarations of that key with different placeholder schemas are registration
  errors.
- Walk and validate into temporary data before committing registration.
  A failure must not partially register a definition or consume message slots.
- Register unresolved declaration copies for each application. Never trust a
  nonzero index imported from another application or persist it to disk.

## Definition-wide translation domains

A domain separates translations for application areas such as `todos`,
`settings`, and `showcase`. Message identity is
`(domain, source text, optional context)`: identical keys share one application-
wide index, while identical text in different domains receives separate indices
and can have different translations.

Declare the domain on the definition:

```rust,ignore
UiDefinition::new("todo", tree)
    .with_translation_domain("todos")
```

Default the domain to the definition name for ergonomic, isolated definitions.
Validate explicitly supplied domains as nonempty names. Multiple definitions
may explicitly select the same domain to share translations. Domains are not
derived from slices: UI composition and application data ownership can differ.
Changing a defaulted definition name also changes its catalog domain; use an
explicit domain when it must remain stable.

Registration applies the definition's domain to every reachable i18n expression,
including nested placeholder messages and declared native window properties.
Expressions do not contain a domain field and cannot override it. Reusing a
component or expression in another definition uses that definition's domain.
There are no subtree overrides or implicit cross-domain fallback.

Export one catalog per domain and language, for example
`translations/todos/de.po` and `translations/settings/de.po`, with
`translations/todos/messages.pot` as the source template. The import/export API
takes an explicit domain alongside the adapter, keeping PO `msgctxt` available
for semantic context. A standalone PO file does not need to encode its domain;
the caller supplies it when importing.

Installing or replacing a catalog affects only its selected domain/language
pair, preserving other domains' translations. Domain catalogs populate the same
application-wide indexed language vectors, so evaluation adds no domain lookup.
Missing entries fall back directly to source text.

## Templates and evaluation

Use `{name}` for a named placeholder, and `{{` / `}}` for literal braces.
Restrict placeholder names to ASCII identifiers. Require each distinct source
placeholder to have exactly one argument binding and reject extra bindings.
Repeated occurrences of a placeholder are valid.

Compile templates into literal and argument segments at registration/import.
Translations can reorder and repeat placeholders, but must preserve the source
placeholder-name set. Validate malformed braces, unknown/missing names, and
duplicate bindings before installation. No escaping language, executable
expressions, or markup is accepted inside translated strings.

Evaluation:

1. Require application context and the render's selected language.
2. Resolve the registered expression's message index and select the language's
   compiled template, falling back to its compiled source.
3. Evaluate each argument expression once, in declaration order, against the
   same current value and language context. Nested i18n expressions work.
4. Convert supported scalar values explicitly and append compiled segments.
5. Return an owned reflected `String` in `DynamicObject<'a>`.

Start with `String`, `PixuiString`, booleans, and integer/floating scalar types;
document their locale-independent formatting. Reject sequences and arbitrary
objects with a useful error. Do not use debug output as user-visible formatting.
Bound nesting depth, template size, and expanded output size to avoid runaway
allocation. Failed evaluation follows existing last-good-render behavior.

## Registry, language selection, and lifecycle

- Define opaque typed message/language index wrappers and checked lookups.
  An invalid/unresolved message index is an error, not an accidental lookup at
  another slot. Out-of-range language indices are rejected at UI creation or
  presentation updates.
- Configure a source-language tag and register translation language tags once.
  Resolve language tags on configuration; evaluate with indices only.
  Runtime IDs are application-local and must not be serialized as catalog IDs.
- Keep `locale` for existing custom resolvers and future formatting. Provide a
  convenience API that resolves a registered language tag and sets both locale
  and language index. Document that manual changes can intentionally differ;
  default source language does not silently infer a catalog from `locale`.
- Carry the selected language in `ExpressionContext` for render/props/title
  evaluation and preserve it through `with_value` when entering loop elements.
  Non-UI evaluation defaults to source language unless explicitly selected.
- Assign new message slots in registration order. Add empty slots to every
  language vector when messages are appended. Keep indices for the application
  lifetime, including after definitions are removed.
- Retain imported catalogs by stable key as well as compiled indexed vectors.
  This allows catalogs loaded before a later definition to fill new slots.
  Lookups on the render path use only the vectors.
- Catalog installation/replacement is atomic on the worker. Compile and validate
  first; retain the previous domain/language catalog on error. Missing entries
  are allowed.
  Report unmatched entries separately so partial catalogs remain usable.
- Language or catalog changes invalidate affected rendering and native metadata,
  including metadata while windows are hidden. Existing published outputs own
  their strings and remain valid. Do not change draw commands or GUI rendering.
- No locks, filesystem reads, or format parsing during expression evaluation.
  Catalog updates use the existing application dispatch mechanism.
- Start with exact language selection followed by source fallback. Regional
  fallback chains such as `de-AT → de → source` can be added explicitly later.

## Catalog identity and extraction

A catalog entry belongs to a domain and contains source, optional disambiguating
context, placeholder
names, translator comments, and available declaration locations. Use context
where identical words need different translations, such as noun/verb “Open”.
Source changes intentionally create a new key; translator tools can assist
migration. Optional persistent developer message IDs can be considered later.

Offer application APIs to enumerate/export a catalog snapshot and a showcase
CLI command, for example:

```bash
./t cargo run -p pixui-example-showcase -- --export-translations /tmp/showcase.pot
```

The command registers the same complete definitions as normal startup but does
not create UI instances, rasterize fonts, start the GUI, or require a display.
Refactor setup so registration/export does not depend on image decoding.
Filter exports by domain and sort entries by context/source; preserve Unicode
and multiline strings,
escape the target format correctly, and avoid volatile timestamps so exports
are reproducible. Export all branches and loop templates exactly once.

## Translation format options

| Format | Tooling and benefits | Costs / limitations | Recommendation |
| --- | --- | --- | --- |
| GNU gettext POT/PO | POT source extraction, PO translations, contexts/comments, established gettext workflow and Weblate support | Source text is identity; custom brace interpolation needs our own validator; full gettext plural behavior is outside the initial scope | First adapter |
| Fluent FTL | Named variables, selectors, plural categories, reusable terms; Weblate support | A richer runtime language, not just a serialization format; full support needs evaluation beyond compiled literal/argument segments | Revisit when language-sensitive grammar is required |
| XLIFF | Standard translation interchange, source/target units and metadata; useful with translation vendors | Verbose XML, inline placeholder handling, and significant version/profile differences between tools | Add on actual workflow demand |
| JSON keyed catalog | Easy inspection and straightforward Rust serialization; Weblate supports several JSON conventions | No single standard schema for contexts/comments/placeholders; hand-rolled schema adds translator friction | Useful internal/debug option; not the primary interchange |

For XLIFF, choose a specific version/profile only after checking the intended
translation tool; support for XLIFF 1.2 does not imply support for 2.1.

### Recommended first PO workflow

- Export `msgctxt` for context, `msgid` for source, extracted comments for
  placeholder descriptions, and `msgstr ""` in the POT.
- Import source-keyed `msgstr` values into the neutral catalog. Ignore fuzzy
  and obsolete entries with diagnostics. Treat empty `msgstr` as missing.
  Intentionally blank translations are deferred rather than conflated with
  untranslated entries.
- Read multiline/escaped PO syntax and UTF-8 metadata correctly; use an existing
  Rust parser if it meets the required round-trip and license constraints.
- Reject plural entries in this initial adapter with an explicit unsupported
  diagnostic. Do not silently select one plural form.
- GNU gettext tools do not automatically validate our custom `{name}` grammar;
  the adapter validates placeholders against source declarations.
- Demonstrate German translations for the showcase. Use count-neutral messages
  such as “Open todos: {count}”; interpolation alone is not plural support.

### Pluggability boundary

Define neutral catalog records and a small explicit `TranslationFormat` trait:

```rust,ignore
trait TranslationFormat {
    fn import(&self, input: &str) -> PixuiResult<TranslationCatalog>;
    fn export(&self, messages: &[MessageDeclaration]) -> PixuiResult<String>;
}
```

Callers select the domain and pass an adapter directly; adapters operate on one
domain's messages at a time. No global plugin registry, dynamic library
loading, or runtime trait object per message is needed. Keep adapters in named
modules with no third-party types in the public catalog model. The application
accepts neutral catalogs and installs vectors independently of their origin.

This makes PO/JSON/XLIFF interchange replaceable. It does not promise lossless
conversion of Fluent selectors/terms or gettext plurals into a simple string
template. A future rich message evaluator is a separate design decision.

## Implementation checklist

- [ ] Define message/language indices, declaration and catalog types, and
      template syntax/error contracts.
- [ ] Add i18n expressions, recursive declaration traversal, compiled
  interpolation, and owned-string evaluation.
- [ ] Add expression-backed component and window-property bindings that expose
  messages to registration.
- [ ] Add definition-wide domains, name defaults, validation, and domain-scoped
  catalog installation/export with no per-expression or subtree overrides.
- [ ] Add transactional definition registration and application-wide
      deduplication; reserve zero and support append-only late registration.
- [ ] Add selected language to presentation and expression context, including
  nested loop propagation and checked configuration helpers.
- [ ] Add catalog installation/replacement APIs and worker invalidation.
- [ ] Add the format trait and PO/POT adapter with reproducible export.
- [ ] Convert showcase source strings, including conditional text and title, to
  declared expressions; load a German PO catalog. Translate UI labels rather
  than shared user data.
- [ ] Add showcase export CLI and document translator workflow. Convert todo's
  existing locale branches where the declaration API supports them.
- [ ] Update architecture documentation and expression/UI API documentation.
  Record the catalog identity and format/evaluation separation in a decision
  record if adopted.
- [ ] Run verification and manual language-switch checks; run `./n check`.

## Verification

- Registration: zero-to-resolved transition, deduplication across definitions
  and different argument expressions, context disambiguation, conflicting
  schemas, failure rollback, cross-application reuse, and late definitions.
- Domains: default and explicit names, sharing within a domain, isolation across
  domains, inheritance in nested expressions and native titles, domain-filtered
  exports, and replacing one domain catalog without changing another.
- Extraction: hidden match arms, empty loop bodies, nested placeholder messages,
  component props, native titles, comments, and deterministic repeated export.
- Interpolation: reordering, repeated arguments evaluated once, nested messages,
  escaped braces, Unicode/multiline text, zero arguments, invalid templates,
  mismatched names, unsupported values, and resource limits.
- Languages/catalogs: two instances with distinct language indices, fallback for
  absent/partial catalogs, invalid indices, catalog-before-definition loading,
  atomic failed replacement, source edits, and runtime language switching.
- PO adapter: actual PO/POT fixtures for context, escapes, multiline strings,
  metadata, fuzzy/obsolete/empty entries, unsupported plurals, unknown entries,
  and import/export round trips.
- End-to-end: German showcase texts and title, source fallback, unchanged shared
  action data, hidden-window metadata updates, and no GUI/font work in export.
- Native smoke test: both themes, live counter placeholders, keyboard actions,
  runtime language switching, and last-good-frame behavior on errors.
- Run `./n check` after the implementation and each corrective unit of work.

## Open questions and limits

1. Confirm PO/POT as the first format and strict placeholder-name-set
   validation.
2. Confirm domain/source/context identity instead of mandatory developer IDs.
   Renaming source text requires catalog updates; comments alone cannot
   disambiguate keys.
3. Should intentionally empty translations be supported immediately? Proposed:
   empty PO translations mean missing in the first version.
4. Are plural-sensitive messages needed in the first release? If yes, choose
   plural semantics before committing to literal/argument-only templates.
5. Should language selection replace `locale` entirely? Proposed: retain locale
   for compatibility and future formatting, with a helper that updates both.
6. Current character-atlas rendering does not support general shaping or bidi.
   Translation support must not be described as complete support for every
   writing system. Longer strings also expose current fixed-row/no-wrap limits.

## Sources

- [GNU gettext manual](https://www.gnu.org/software/gettext/manual/gettext.html)
  describes PO/POT entries, workflows, plural entries, and fuzzy translations.
- [GNU gettext contexts](https://www.gnu.org/software/gettext/manual/html_node/Contexts.html)
  explains disambiguating identical source strings.
- [Fluent syntax guide](https://projectfluent.org/fluent/guide/index)
  and [variables](https://projectfluent.org/fluent/guide/variables.html)
  describe variables and language-sensitive variants.
- [Weblate format support](https://docs.weblate.org/en/latest/formats.html)
  documents supported formats and format-specific metadata.
- [OASIS XLIFF 2.1](https://docs.oasis-open.org/xliff/xliff-core/v2.1/xliff-core-v2.1.html)
  specifies the interchange format.
