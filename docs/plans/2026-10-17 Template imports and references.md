# Template imports and references

Status: implementation in progress.

Accepted naming: `ComponentRef` and `<Dynamic component=... />`. Host-defined
titles use the proposed `LocalizedText` handle.

## Goal

Compose `.pixui` files and render reusable templates selected through reflected
application data. The showcase should declare its navigation entries once in
Rust, loop over them to draw the sidebar, and render the selected entry's
reference. Remove the host's positional insertion into empty match arms.

A template reference identifies a reusable UI subtree, not a registered leaf
component/painter or a mounted instance. Use `ComponentRef` to keep those
concepts separate. Existing component registration and props construction retain
their current meaning.

## Proposed usage

Static source composition:

```text
import Buttons from './buttons.pixui';

ui Example domain 'showcase' {
    <Column>
        <Buttons />
    </Column>
}
```

Each file still declares one `ui`; the import gives it a local alias. The host
may export a Rust-built template under the same kind of alias. Static tags are
sugar for rendering that fixed template reference.

Data-driven composition:

```rust
// Illustrative API; errors and setup omitted here.
#[pixui_reflect::reflect]
struct NavigationEntry {
    pub id: u64,
    pub title: LocalizedText,
    pub component: ComponentRef,
}

let entries = vec![
    NavigationEntry {
        id: 0,
        title: app.localized_text("showcase", "Buttons")?,
        component: compiled.template("buttons.pixui")?,
    },
    // Other source templates and a Rust-composed Styles template.
];
```

The real reflection attribute currently applies to inline modules; the sketch
above describes the record shape rather than proposing new derive syntax.
`compiled.template` denotes a source-bundle lookup, not a filesystem read during
rendering. Exact public API names should be settled in the first implementation
step.

```text
ui showcase domain 'showcase' {
    <Column slice='showcase'>
        <Column width=200 gap=8>
            {for entry in entities.entries =>
                <Button label=entry.title layout=layouts.full
                        @activate=select(id: entry.id) />
            }
        </Column>
        <Dynamic component=entities.selected.component />
    </Column>
}
```

`select` validates the entry ID and updates a selected-entry entity on the
worker. Keep stable IDs separate from list position. This requires no new
indexing operator, optional-value syntax or runtime source compilation. Select
the first valid entry at startup; reject unknown IDs without changing selection.

## Design

### Source bundles and imports

- Add a host-supplied source resolver: importer identity plus relative specifier
  returns a canonical module identity and an immutable source snapshot. The
  compiler never opens arbitrary paths itself. Supply an in-memory bundle for
  the showcase's `include_str!` sources and tests; filesystem adapters can
  follow.
- Resolve relative imports against the importing module. Normalize module IDs,
  reject escape above the bundle root, and use canonical IDs for caching and
  cycle detection. Keep this distinct from `ResourcePath`: UI modules need
  relative `./` and `../` resolution, whereas image paths forbid dot segments.
- Parse imports before the `ui` declaration. Imports use untranslated literal
  filenames without interpolation. Reject duplicate aliases, unknown modules,
  collisions with component aliases and reserved tags (`Row`, `Column`, `Grid`,
  `Dynamic`). Do not add wildcards, re-exports or package discovery initially.
- Compile the reachable graph once per bundle/environment, using DFS to report
  cycles with the import chain. Share immutable compiled templates rather than
  repeatedly expanding imported trees. Bound module count, graph depth and
  aggregate source bytes in addition to existing per-file compiler limits.
- The bundle API may take multiple entry modules so Rust can build a registry of
  pages even when the shell does not statically import them. Only successfully
  checked bundles are installed; failed compilation publishes no partial
  registry.
- Retain the existing single-source compilation API for files without imports.
  Imports through that API report that a resolver/bundle is required.

### Template references and registration

- Introduce an opaque, cheaply cloned `ComponentRef` with cached reflection and
  an ownership adapter. It exposes no constructor or writable identity fields.
  It must work inside reflected structs, vectors, entities and collections.
- References are application-owned and validated against an application
  identity. Never accept an arbitrary integer or source filename as a runtime
  reference. No ownership cycles through reference-counted live trees: prefer
  registry-owned templates and IDs/handles into that registry.
- Provide explicit registration for Rust-built `LivePart` templates and compiled
  bundles. They produce the same reference type and use the same render path.
  Installation validates referenced components, styles and dependencies.
- For this milestone, registrations live until application shutdown.
  Replacement, unregistration, template hot reload and reference recycling are
  separate work. Document this lifetime rather than adding a speculative release
  protocol.
- Register every template's declarations before it is first selected.
  Translation export must include inactive pages. Iterate shared dependencies
  once, reject foreign references and avoid repeatedly registering messages on
  every frame.

### Static and dynamic rendering

- Add a typed live-part node for a template reference expression.
  `<Dynamic component=... />` requires exactly `ComponentRef`;
  `<ImportedAlias />` lowers to the same mechanism with a constant reference. No
  string lookup on the paint path. Reserve the `Dynamic` tag in host component
  exports.
- Add no template props, children/slots, callback parameters or arbitrary
  factory calls yet. Reject extra attributes/children with a source-located
  explanation. Put layout constraints on a surrounding container when needed.
- Resolve the reference on the worker during preparation/walking. Selecting the
  same reference retains subtree state. Changing it unmounts the old subtree and
  initializes the new one with `PartState::Unknown`, matching current page-match
  behavior. Switching back starts fresh component-local state; application data
  remains unchanged.
- Reuse immutable template definitions, while maintaining state per occurrence.
  Two uses of the same reference never share component-local state accidentally.
  Add an identity segment for the template boundary/reference so switches cannot
  transfer focus, hover, editing sessions or pending callbacks to unrelated
  nodes.
- Extend every relevant traversal: state walk, preparation, retained layout
  reconciliation, painting/input IDs, translation registration and diagnostics.
  Preserve per-instance geometry and definition-shared interaction behavior.
- Imported templates resolve names and action slice defaults lexically in their
  own module against the shared host environment. They cannot capture a caller's
  loop locals, event parameters or ambient slice directive. The
  selected-reference expression may read caller locals; evaluating the callee
  starts a fresh local scope. Templates should declare their own `slice` where
  unqualified actions need it. Keep presentation/language and owning definition
  context.
- Static imports must be acyclic. Dynamic references also need an
  active-template stack and a maximum expansion depth/node budget to reject
  recursive rendering, including Rust-built cycles. Failure retains the last
  published output and geometry; no stack overflow or partially installed
  subtree is allowed.

### Titles and translations

A Rust `String` read from a row is application data and is not automatically
translated. Moving sidebar titles into that form would regress the German UI.

- Introduce a narrowly scoped opaque `LocalizedText` handle for messages
  declared by host code. The host registers source text and a definition-wide
  domain; declarations are exportable before a window is opened.
- In a string-valued prop/template position, resolve this handle using the
  current presentation language with the existing untranslated fallback. Each
  window receives its own string; do not store a localized string back into
  shared data.
- Use the existing i18n registry and message identity rules, not a second
  catalog system. Support simple messages for this milestone; parameterized host
  message values can follow when needed. No implicit translation of arbitrary
  Strings.
- Template modules inherit the owning definition's translation domain. An import
  is not a new translation region. Keep standalone `ui ... domain ...` behavior;
  embedded module domains must be compatible with the owner or fail clearly.
- Validate host title domains against the owner on installation so export and
  rendering agree. Document the same-template/multiple-domain restriction now;
  do not silently reuse translation indices across incompatible domains.

### Diagnostics and source locations

Imports, aliases, references and rendering nodes all carry spans in their own
source snapshots. Missing imports point at the import string; duplicate aliases
show both declarations; cycles show the participating import sites. Checking
errors retain the imported filename and may add the importing call site.
Runtime foreign-reference, recursion and subtree errors include the render site
and preserve the original callee error location. Host-only registrations report
their registered name when no source span exists.

## Showcase migration

- Compile all pane sources as one bundle with the common host environment.
- Register the remaining Rust-composed Styles page through `ComponentRef` as
  well; leave its dynamic-style button and native title/icon adapters in Rust.
- Create a reflected navigation-entry vector with stable IDs, localized title
  handles and template references. Bind it and an initialized selected entry in
  the showcase slice. Update `select` and the generated action facade to take
  IDs.
- Replace repeated sidebar declarations with a loop and the empty-arm match with
  `<Dynamic component=entities.selected.component />`. Preserve the fixed
  sidebar, header/footer, full-width buttons and page ordering.
- Remove positional `assemble_pages` logic and duplicate page-count assumptions.
  Retain source files for each pane; static-import fixtures should separately
  demonstrate direct `<Alias />` composition as the showcase uses dynamic refs.
- Preserve translations, focus behavior, resource sharing, style examples,
  layout stress loops, controlled text and synchronized selection in both
  windows.

## Implementation checklist

- [ ] Settle API names and the title-localization decision below; document the
  boundary between registered leaf components, templates and mounted instances.
- [ ] Add source-bundle/resolver APIs, import AST/spans, graph loading,
      canonical module identity, duplicate/cycle diagnostics and resource
      limits.
- [ ] Add the template registry, opaque reflected references, Rust registration
  and atomic compiled-bundle installation.
- [ ] Resolve template aliases and type-check/lower static and dynamic
      rendering.
- [ ] Add template-boundary state/identity handling and update all runtime and
      declaration traversals, including recursion/budget checks and failure
      recovery.
- [ ] Add host-declared localized titles and presentation-time string resolution
  through the existing i18n registry.
- [ ] Migrate showcase navigation/selection and remove positional Rust
      composition.
- [ ] Add tests listed below; run `./n check` after each unit of work.
- [ ] Update the language reference, example README, architecture documentation
  and any affected state/identity documentation.
- [ ] Manually validate both showcase windows, navigation, translations, focus,
  page switching, styles, images and text input before completing the plan.

## Verification

- In-memory import graphs: relative imports, shared dependencies, alias/tag
  collisions, missing modules, cycles, root escapes and graph limits.
  Diagnostics point to the correct file/span. Ordinary single-file compilation
  still works.
- Static and reflected references render the same Rust/source subtrees. Cover
  nested composition, refs in loop fields/entities/collections, type errors and
  cross-application rejection. No filesystem reads occur during rendering.
- Repeated references share definitions but retain independent occurrence state;
  unchanged refs retain state and switched refs reset it. Exercise nested loops,
  matches, retained layout, focus/hover cleanup and stale input/clipboard
  replies.
- Dynamic recursion and excessive expansion fail deterministically, keep the
  last successful frame/geometry and recover after correcting the selection.
- Export includes untranslated source messages from inactive imported templates
  and host titles exactly once. English/German windows resolve the same title
  handle differently; titles and template contents use the owning domain.
- Existing showcase tests continue to cover all panes, event dispatch, footer
  and sidebar geometry, translation export, styles, images, nested layout and
  text. Add a navigation entry in a fixture without editing shell markup and
  verify its button selects the corresponding template in both windows. Invalid
  IDs leave selection intact; actions/state survive page switches as before.
- Native verification: resize both presentations, switch every pane by mouse and
  keyboard, verify German titles, controlled editing and dynamic Styles
  behavior.

## Open questions and assumptions

1. **Names:** recommend `ComponentRef`, `LocalizedText` and
   `<Dynamic component=... />`. Calling both leaf components and whole templates
   “components” is convenient conversationally, but their Rust types and
   language diagnostics should differ.
2. **Title localization:** recommend the small host-declared message-handle API
   above. Alternative: move navigation metadata into source so titles use
   ordinary i18n expressions. That avoids a new value type but does not satisfy
   a fully Rust-defined list of titles/references without another
   data-declaration feature. Confirm before implementing the localization
   portion.
3. **Scope:** no template parameters/slots, import-specific environments,
   automatic filesystem watching or runtime compilation. The reference mechanism
   should not accidentally promise React-style parameterized components yet.
4. **Identity:** positional loops remain positional under existing rules. Stable
   entry IDs select pages but do not automatically key loop occurrences. Keep
   showcase ordering fixed; source-level keyed-loop syntax is separate work.
5. **Lifetime:** append-only application registry is a deliberate first version.
   Track template count/definition memory in tests; do not register fresh
   templates from every action/render. Revisit cleanup when runtime template
   creation or hot replacement is an actual requirement.
6. **Compatibility:** imported templates share one host environment and
   definition domain. Reuse across incompatible domains/environments requires
   explicit recompilation/registration, not mutation of existing registered
   templates.
