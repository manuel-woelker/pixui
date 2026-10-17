# Reusable composite components

Status: complete; moved to completed at user request.

## Goal

Define reusable UI structure and layout in `.pixui`, with typed props and an
optional child slot. Components may be private to a file or exported by name and
used from another file. Refactor the showcase Layout pane to exercise the
feature without changing its appearance, actions, translations or physical
layout.

Use the existing source bundles, component registry, occurrence identities and
worker rendering pipeline. Composite components remain distinct from registered
Rust leaf component types and painters.

## Proposed syntax

Assume `export` marks public declarations and `use` imports named exports:

```text
// ui/common.pixui
export component Card(children: Children) {
    <Column min_width=0 padding=8 gap=4>
        <Children />
    </Column>
}

export component MetricCard(title: String, value: String) {
    <Card>
        <Label text=props.title />
        <Label text=props.value />
    </Card>
}

export component ActionCell(children: Children) {
    <Column basis=140 grow=1 min_width=140>
        <Children />
    </Column>
}
```

```text
use { MetricCard, ActionCell } from './common.pixui';

ui LayoutDemo domain 'showcase' {
    <Column slice='showcase'>
        <Grid columns='1fr 1fr 1fr' rows='72px 72px' gap=8>
            <MetricCard title="Projects" value='12' />
            <MetricCard title="Tasks" value='48' />
        </Grid>
        <Row gap=8 wrap=true>
            <ActionCell>
                <Button label="Plan sprint" @activate=increment() />
            </ActionCell>
            <ActionCell>
                <Button label="Create task" @activate=increment() />
            </ActionCell>
        </Row>
    </Column>
}
```

Private declarations use `component Name(...) { ... }`. A module may have
multiple component declarations and at most one existing `ui` entry definition;
a library module need not have a `ui`. Named imports support explicit aliases:

```text
use { Card as Panel } from './common.pixui';
```

`export component` is the only export declaration initially. Do not introduce
`export as` as a combined keyword, implicit exports, wildcard imports,
re-exports or namespace imports. This is the selected implementation syntax.

## Props

- Props have explicit types and are all required. Start with existing primitive
  types, `String`, `ResourcePath` and `LocalizedText`; resolve names through an
  explicit type table. Do not expose arbitrary Rust types by spelling their
  internal type names. Host type aliases may be added through a documented
  export API when the existing ownership/reflection adapters support them.
- Use `props.name` inside a component; props do not become implicit bare names.
  Reserve `props` as a namespace. A nested component sees its own props, while
  projected children retain access to the caller's props.
- Attribute order is independent of declaration order. Reject unknown,
  duplicate, missing and incompatible props at the call site, with declaration
  labels when useful. Apply existing contextual numeric/path/localized-text
  rules according to the receiving type; never introduce blanket String
  conversions.
- Evaluate each supplied prop expression once per reached occurrence per walk,
  before entering the component body. Keep evaluation results in a checked,
  field-ordered invocation frame. Refresh values on later walks without
  resetting component-local state merely because a prop value changed.
- Use existing ownership adapters for values crossing this boundary. Document
  the Clone/Send/Sync requirements for host types; do not keep borrows in
  mounted state or introduce application-state locks. Evaluation failures retain
  the last published frame and geometry, following current preparation
  semantics.
- Defer defaults, optional props, generics, spreads, arbitrary expression types,
  custom composite events and callback-valued props. Child slots let callers
  provide buttons/events without requiring event forwarding in this milestone.

## Child slot and lexical scope

- `children: Children` declares one optional ordered child slot. Zero children
  means an empty fragment; one or more children preserve their source order.
  Components without this declaration reject children.
- `<Children />` is valid only in a component declaring that slot. It accepts no
  attributes or nested children. Initially allow at most one placeholder per
  component body; reject multiple placeholders rather than mounting supplied
  content several times accidentally. A placeholder may be inside a loop, in
  which case each reached occurrence has independent physical state.
- Projected content resolves names, outer props, loop variables, event bindings
  and the default action slice at the call site. The component body resolves
  names, imports and its default slice at the declaration site. A wrapper's
  slice directive must not silently retarget caller-supplied actions.
- Keep the current behavior for ordinary composite calls: they do not capture
  caller loop locals implicitly. Only explicit prop expressions and projected
  children may access caller context.
- Do not store borrowed caller contexts beyond the walk. Prefer reusing scoped
  evaluation frames during preparation; callback bindings still capture owned
  snapshots through the existing event mechanism. Use a distinct slot boundary
  in identity paths and retain the active component stack/expansion budget when
  restoring caller lexical context, so slots cannot bypass recursion checks.
- An invocation and its projected content do not add layout boxes. Only explicit
  containers in the body/slot participate in Taffy. Keep layout on surrounding
  containers or inside wrappers; no automatic forwarding of `layout`, `styles`,
  `focus`, `slice` or event attributes onto a composite's root.

## Modules, exports and compatibility

- Replace the single-definition module AST with a spanned module containing
  `use` declarations, component declarations and an optional `ui`. Preserve
  spans on parameter names/types, export markers, import members/aliases, prop
  accesses and slot nodes through checking and lowering.
- Build local and exported symbol tables before checking bodies so forward local
  references work. Imports expose only named exports; private declarations stay
  inaccessible. Diagnose duplicate declarations, ambiguous aliases and
  collisions with host leaf aliases/reserved tags (`Row`, `Column`, `Grid`,
  `Dynamic`, `Children`) instead of choosing one silently.
- Reuse canonical module IDs and existing bounded source-graph loading. Detect
  cycles in the component declaration graph as well as module import cycles;
  preserve runtime recursion/node guards for dynamic or Rust-composed
  references. Bound declaration/parameter counts and total checked bodies as
  part of the bundle budget, so a many-component file cannot bypass existing
  limits.
- Check all declarations in an installed module, including unused private ones.
  Register exported declarations and their required dependencies once; never
  inline a new definition tree for every static call or compile during painting.
- Expose named exports through the installed-bundle API, for example
  `bundle.export(module_id, name)`. Preserve `bundle.component(module_id)` for a
  module's optional `ui` entry; report a clear error for library-only modules.
- Keep existing `ui` declarations and single-file compilation working.
  Single-file compilation supports local composites; external `use` still
  requires a resolver.
- Recommended migration: switch this repository's old
  `import Alias from './file.pixui';` syntax to `use` and reject the old syntax
  with a targeted migration diagnostic. For importing a file's `ui` entry, use
  `use Alias from './file.pixui';`; named imports select explicit exports.
  Update fixtures/docs together. Avoid maintaining two equivalent import
  keywords indefinitely. This compatibility policy is selected for
  implementation.

## Runtime and component references

- Extend the existing reusable-component descriptor with a checked prop
  signature and slot declaration. The language environment needs this metadata
  to validate imported calls without executing application code.
- Lower static composite calls to the existing component boundary mechanism plus
  prop evaluators and an optional caller-owned slot template. Keep definition
  trees shared and invocation/mounted state separate.
- Retain state while the referenced definition and call-site occurrence remain
  the same. Prop changes update prepared props, not identity. A changed
  reference or removed occurrence unmounts its state, including focus/editing
  ownership, using the current `ComponentRef` boundary behavior.
- Existing zero-argument `ComponentRef` navigation and
  `<Dynamic component=... />` remain supported. Recommend deferring arbitrary
  dynamic prop bags: a reference to a component with required props or a child
  slot must report an explicit unbound-invocation error when rendered through
  the zero-argument Dynamic API. Static calls can supply those arguments with
  full checking. Document how named export metadata and opaque references
  differ; do not silently drop arguments or fabricate defaults. Bound references
  can be a later extension.
- Resolve prop reads by checked indices, not a name lookup per frame. Extend all
  relevant traversal/validation paths: walking/state reconciliation, layout,
  input targeting, declaration/i18n registration and deferred callback capture.
  Inactive composite definitions remain validated and translatable.

## Translation and diagnostics

- Literal text at the call site remains an i18n declaration there; reading a
  `String` prop does not translate it a second time. Preserve single-quoted
  untranslated templates, structural resource paths and `LocalizedText`
  behavior.
- Shared component bodies use the owning definition's translation domain, as
  current imports do. A library does not introduce a new domain. Register body,
  invocation-prop and slot declarations before rendering, including inactive
  calls/branches. Keep source locations in the declaring/calling file and dedupe
  repeated component-body messages through the existing registry.
- Errors name the component and prop/slot involved. Call-site diagnostics can
  point to a declaration in another file; extend the diagnostic label model if
  needed to retain each label's own source snapshot. Runtime failures include
  the invocation chain while preserving the original expression's source span.
- Preserve transactional bundle installation. Invalid exports, signatures,
  invocations or declarations publish no partial references or translations.

## Showcase migration

Create a library under `examples/showcase/ui/` containing `Card`, `MetricCard`
and `ActionCell` (private helpers where appropriate). Use it from `layout.pixui`
to remove repeated metric-card and wrapping-action markup. Consider a task-row
wrapper only if it removes meaningful repetition without hiding layout intent.

Keep the existing six metrics, six action buttons, 36 project cards and nested
six-task loops. Do not change data structures just to demonstrate props.
Preserve page IDs, translated labels, component order, click behavior, wrapping
widths and footer/sidebar geometry so existing layout/input tests remain
meaningful.

## Implementation checklist

- [x] Confirm `export`/`use` syntax, prop/slot scope, dynamic-reference limits
      and import migration policy below.
- [x] Add module/declaration/parameter/use/slot AST nodes and spans; extend
      parser diagnostics, recovery and limits without changing expression
      ambiguities.
- [x] Add local/export symbol tables, named/aliased imports, signature checking
  and component-cycle detection with cross-file declaration labels.
- [x] Add indexed prop frames, caller-scoped child projection and invocation
  lowering; extend registry/environment metadata while retaining zero-arg refs.
- [x] Update mounted state/identity, callback capture, layout/input traversal,
  domain validation and atomic declaration/i18n registration.
- [x] Migrate the showcase Layout pane and old import fixtures; update language,
  architecture, state/identity and example documentation.
- [x] Add the tests below and run `./n check` after each unit of work.
- [x] Close the plan and move it to `completed` at user request. Native
  showcase validation was requested; no separate result was recorded.

## Verification

- Parse multiple/private/exported components, library-only modules, local
  forward calls, named/aliased `use`, optional UI entries and precise
  spans/recovery.
- Reject private/missing exports, duplicate symbols/props, reserved aliases,
  missing/unknown/mistyped props, illegal slots, old syntax and
  declaration/import cycles. Check both call and declaration locations across
  files.
- Verify props evaluate once, change across renders, shadow correctly and use
  numeric, resource-path and localized-text rules. Exercise props inside loops,
  conditionals and deferred callbacks, without accidental caller-local capture.
- Verify empty/multiple child fragments, nested projection, caller loop/props
  and action-slice capture, conditional/loop slot occurrence state, and
  resource/error recovery. Confirm traversal returns to the correct caller scope
  afterward.
- Verify independent mounts, state retention on prop changes, cleanup after
  removal/ref switches, stale focus/input/clipboard rejection and expansion
  bounds.
- Verify zero-argument dynamic references still work and parameterized unbound
  references fail clearly. Invalid bundles leave no registrations/messages
  behind.
- Export body/prop/slot messages from inactive uses; compare English/German
  output and source locations without double translation or resource-path
  localization.
- Retain existing showcase numerical layout, wrapping, actions and performance
  fixtures. Native validation covers resize, keyboard/mouse navigation, text
  editing, translated metrics and dynamic Styles in both windows.

## Selected decisions and limits

1. **Keywords:** selected `export component` and `use`; selected for
   implementation. `as` is an alias modifier, not an export keyword.
2. **Import migration:** selected `use Alias from 'file'` for existing UI
   entries and `use { Name as Alias } from 'file'` for named component exports,
   replacing `import` with a helpful error. No compatibility alias by default.
3. **Props:** selected required, typed props with explicit `props.` access; no
   defaults or callback-valued props yet. `children: Children` opts into one
   optional slot and no automatic root-attribute forwarding.
4. **Dynamic calls:** selected keeping Dynamic zero-argument initially rather
   than introducing unchecked prop maps. A future bound-reference API can
   capture checked arguments/slots when a concrete code-driven use case needs
   it.
5. **Scope:** no local component state declarations, inheritance, CSS selectors,
   named slots, re-exports, filesystem watching or new layout algorithm. Reuse
   existing application state, leaf state, i18n and source-resolver mechanisms.

## Implementation notes

- Plan committed before implementation as `63dca95`.
- Selected the recommended syntax and compatibility policy: `export component`,
  named/default `use`, explicit `props.` fields and one optional child slot.
  Existing `import` syntax receives a targeted migration error.
- Static source calls share checked definition DAGs. Mounted state and caller
  slot templates remain per occurrence; prop values and caller contexts are
  scoped to each walk. Finite nested uses of the same wrapper are allowed;
  source cycles and expansion depth/node limits remain enforced.
- Named export metadata supports checked static calls; its installed
  ComponentRef retains the existing zero-argument Dynamic boundary. Required
  props/slots are rejected when the reference is rendered unbound.
- Implemented module ASTs and named/default `use`, private/exported
  declarations, typed indexed props, caller-scoped child projection and
  cross-file diagnostics.
- Shared definition DAGs are cached during declaration passes. Calls/slots have
  separate mounted state and occurrence identity; projected callbacks capture
  owned caller values. Source type aliases use existing ownership adapters.
- Added a total-declaration bundle limit and reserved `props` for expression
  frames (including loop/event namespace validation).
- The showcase now imports `MetricCard` and `ActionCell` from `common.pixui`;
  existing two-width layout, action and translation tests pass unchanged.
- Automated `./n check` runs passed for core language/runtime work, named
  imports, showcase migration, callback scopes, prop refresh/state retention,
  localization and resource conversion. Final coverage also wraps worker image
  loading in a composite and checks AST spans. Native validation was requested;
  the user directed completion and moving the plan on 2026-10-17 without
  recording a separate result.
- Explicit limit: container layout remains static. This work reuses fixed layout
  wrappers; it does not add dynamic layout/style resolvers for prop-driven gaps,
  padding or dimensions.
