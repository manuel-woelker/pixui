# Component events and callback expressions

Status: completed.

## Goal

Let components declare typed, named events and emit them from worker-side
behavior. UI definitions bind callback expressions to these events. Support
activation, text changes, mouse clicks and custom component events through one
mechanism, accessible from Rust and `.pixui`.

This removes the language's activation-only limitation and the separate
text-input change binding. It is a prerequisite for converting the showcase's
controlled inputs; image, styling and layout language gaps remain separate work.

## Current implementation

`ComponentPart` has separate activation and change factories, plus a
language-specific compiled activation. The renderer prepares owned bindings;
published instance geometry stores activation targets and text-input targets.
The worker translates low-level `UiInput` into activation or editing behavior.
Change bindings are restricted to `TextInputComponent`. Language action programs
snapshot all request arguments during preparation, so they cannot accept a
payload produced later by input.

Preserve worker ownership, published input revisions, stable component paths,
shared definition-side interaction state and controlled text reconciliation.

## Proposed API and metadata

- Each component declares one associated `Events` type, alongside `Props` and
  `State`. It is a trait object describing event methods. A
  `#[component_events]` proc macro generates descriptors, reflected parameter
  payloads and a proxy implementation. Components without events use a provided
  empty interface.
- An `EventDescriptor` contains a name, payload `TypeDescriptor`, Rust type
  identity and adapters needed to inspect and construct owned request arguments.
  Use reflected payload structs, including an empty struct for activation.
  Payloads are owned, `Send + 'static`; emission itself requires neither `Clone`
  nor `Sync`. Snapshotting fields uses the existing explicit value adapters.
- Each trait method identifies one event; its parameter names become payload
  fields. Resolve names once at registration/compilation, then use indices.
  Generated proxy methods use fixed event indices; validate the component
  registration identity as well as the index. Event names are unique within a
  component, not application-wide.
- A component declaration holds a sparse list of event callback bindings,
  separate from props and painter styles. Bindings are optional; emitting an
  unbound event is a no-op. Reject unknown events and duplicate bindings.
- Component behavior calls ordinary trait methods such as
  `events.change(proposed_content)?`. Provide typed Rust binding helpers using
  generated metadata; erasure occurs inside the registry, never at the
  component author's call site.
- A worker-side event context permits emission and reports input consumption.
  It does not expose a blocking application handle or mutable application state.
  Custom behavior hooks receive typed props, source-instance state and input
  information; global focus/hover/editing state stays in its current shared
  owner. Do not move shared interaction state into these per-instance hooks.

Illustrative Rust declaration:

```rust
#[component_events]
pub trait TextInputEvents {
    fn change(&self, content: String) -> PixuiResult<()>;
    fn submit(&self) -> PixuiResult<()>;
}

impl Component for TextInputComponent {
    type Props = TextInputProps;
    type State = TextInputState;
    type Events = dyn TextInputEvents;
}
```

The associated type permits unsized interfaces. The macro supplies a generated
adapter connecting the trait object to its concrete worker-local proxy; Rust
cannot implement an arbitrary associated trait dynamically. Input hooks receive
`&Self::Events`. Proxy methods enqueue through a `RefCell<Vec<PendingEvent>>`:
no mutex, `Send` or `Sync` requirement on the proxy itself. Queue borrows last
only for appending, and are released before callbacks or actions run.

Initially require object-safe methods with `&self`, owned reflected
`Send + 'static` parameters and `PixuiResult<()>` returns. Reject generic/async
methods, borrowed parameters, default bodies, associated items and other
receivers with macro diagnostics. Zero-argument methods have empty payloads.
Emission results report enqueue/validation failures; action failures occur
later through the worker error path. Method names are event names verbatim.

Illustrative language bindings:

```text
<Column slice='showcase'>
    <Button label="Increment" @activate=increment() />
    <TextInput content=entities.text @change=edit_text(content: content) />
    <ClickArea @click=showcase.record_click(x: x, y: y) />
    <Column slice='ratings'>
        <Rating value=entities.rating @change=set_rating(value: value) />
    </Column>
</Column>
```

The component declares events named `activate`, `change` or `click`; language
attributes use `@` followed by that name. The lexer/parser distinguishes
callback attributes from ordinary props; no `on_` prop prefix is reserved. The
`event` root exists only inside a callback and has that event's payload type. An
event expression initially invokes one permitted slice action using the existing
pure expression grammar. Arbitrary statements, multiple actions and async
callbacks are deferred.

## Named arguments and callback parameters

- Action calls require named arguments: `add(title: 'New todo')` or
  `set_draft(value: content)`. Names refer to reflected action request fields,
  not event parameter names. Allow arguments in any order; resolve field indices
  during checking and construct requests in descriptor field order at runtime.
- Reject missing, unknown and duplicate arguments, positional arguments and
  mixed positional/named calls. Zero-field requests use `action()`. No optional
  arguments or inferred defaults in this iteration. Worker-injected arguments
  such as collections are not request fields and cannot be supplied by callers.
- Event method parameters are implicitly available as local names throughout
  their callback. For `change(&self, content: String)`, both `content` and
  `event.content` access the same payload field. Keep the qualified form for
  clarity; no explicit parameter binding, lambda syntax or renaming form.
- Event parameters shadow enclosing loop locals inside that callback only.
  Preserve lexical loop depths during resolution; report shadowing in a useful
  diagnostic note when an expression fails to type-check. If a needed loop local
  is shadowed, rename the loop variable in the source. The qualified event form
  does not provide access to a shadowed loop local.
- Reserve callback roots `event`, `entities` and `collections`; reject event
  method parameter names that conflict with them. Action callees have their own
  lookup context: an event parameter cannot hide a slice or action name in a
  callback call target. Argument expressions use normal callback-local lookup.
- Parameter locals exist only in callbacks; ordinary props continue to use
  explicit data expressions. Unknown bare names are errors, not string literals.

## Component input values

Component inputs remain ordinary props, separate from emitted events. A
controlled text input receives authoritative content from application state and
proposes replacements through its event interface:

```text
<Column slice='todos'>
    <TextInput content=entities.draft
               @change=set_draft(value: content) />
    <Button label="Add todo" @activate=add(title: entities.draft) />
</Column>
```

`TextInputProps::content` defines the input prop; the event method's `content`
parameter defines the callback local; the action request's `value` field defines
the named argument. These names may differ. Preserve explicit controlled
bindings so actions can validate, normalize or reject edits. No automatic
two-way binding or implicit mutation of props.

## Slice scope and action lookup

- Containers accept `slice='name'` as a static language scope declaration, not a
  layout setting or component prop. Descendants inherit the nearest enclosing
  container's slice. A nested container overrides it for its subtree; siblings
  retain the outer scope. Loops, match arms and fragments preserve lexical
  scope.
- Resolve `slice.action(arguments...)` against that named slice, regardless of
  inherited scope. Resolve bare `action(arguments...)` only against the
  inherited slice; without one, report a compiler error. Do not search other
  slices or fall back to a global action namespace.
- Validate slice names and action availability during compilation. Keep the
  explicit environment allowlist: expose actions under `(slice, action)` names
  and verify supplied handles belong to the indicated application slice. Scope
  never grants access to actions that were not exported.
- Retain resolved handles in runtime callbacks; no slice-name lookup occurs on
  emission. Named slices must be identifiers for dotted calls; aliases may be
  explicitly exported for application slice names that are not identifiers.
- This replaces the `actions.` namespace. Migrate existing source definitions
  and environment setup together; do not retain an ambiguous legacy alias.
- Slice scope affects action lookup only. Existing explicit data exports such
  as `entities.text` and `collections.todos` keep their current meaning.
- Do not add a UI-wide default or implicit application-wide scope. A root
  container provides the default. Leaf declarations use explicit qualified calls
  when needed; component types remain independent of application slices.

## Evaluation and ownership

Use two stages so callbacks remain tied to the UI the user actually saw:

1. During successful preparation, capture owned values and opaque entity refs
   from application data and loop locals. Retain source origins, component path
   and callback identity. No action runs during preparation or painting.
2. On emission, inspect the owned payload, evaluate event-dependent expressions
   using those captures, build one action request and execute it through the
   existing worker dispatch path. Do not enqueue onto the worker's own bounded
   channel or retain borrows beyond callback evaluation.

Capture maximal event-independent expression subtrees so expressions can mix
`event.content` with loop fields or entity values. Pure short-circuit
expressions must retain lazy evaluation: preparation must not eagerly evaluate a
branch whose execution depends on the future payload. Store a deferred program
with captured roots for such branches. Existing contextual numeric typing and
conversion rules continue to apply.

Reuse owned captures across repeated emissions using registered snapshot/send
adapters; each request receives fresh owned arguments. Validate opaque refs at
dispatch, including their generations. Event payloads do not manufacture entity
references from raw collection indices.

Document snapshot semantics explicitly: ordinary application expressions use
the last published preparation, while `event` uses the current emission. An
action handler can read current application data through its normal arguments.
Do not silently reevaluate callbacks against a different tree or locale.

## Input routing and dispatch

- Keep native hosts forwarding low-level input. Route on the worker using the
  source window's published geometry, clipping, focus and input revision.
- Separate raw input handling from semantic event emission. Core button behavior
  emits `activate` for pointer activation and keyboard activation. A click event
  represents a pointer gesture and is not synthesized from Enter or Space.
- Add a default no-op typed component input hook, erased by registration, for
  custom components to translate targeted input into their declared events.
  Input hooks never run from painters or layout measurement callbacks.
- Define direct targeting first: pointer input goes to the topmost eligible
  component under the clip; keyboard input goes to the focused component.
  No ancestor bubbling/capture phase in this iteration. Pointer movement may be
  routed to a component that declares interest without making it focusable.
- Preserve global Tab navigation and F11 policy; focused editing consumes its
  recognized input before generic activation. Document the precedence of global
  shortcuts, built-in editing and custom behavior. Prevent duplicate emissions
  from generic and component-specific handling of the same input.
- Track pointer press identity and button. Emit click/activation only for a
  matching release over the same valid target. Cancel on target removal, lost
  visibility or invalidated input revision. Preserve text selection's existing
  drag/session handling; general drag capture is separate work.
- Process emitted events in order after the input hook returns, before the next
  input message. Hooks emit into a temporary queue, avoiding action reentrancy
  while component state is borrowed. Keep this queue local to one input turn;
  rendering cannot emit events or recursively trigger another input hook.
- A physical event is handled once from its source instance. Shared application
  changes invalidate relevant instances; peer windows do not replay the event.
- On failure, stop processing the remaining emissions from that input turn,
  report callback origin and event name, and resolve actual application state.
  Existing actions do not roll back mutations made before an error.

## Controlled text input

Declare `change(&self, content: String) -> PixuiResult<()>` on the text-input
event trait; the macro generates its reflected payload. Route
current editing proposals through it and preserve all existing semantics:
content remains controlled, read-only inputs still support selection/copy, no-op
edits and IME preedit emit no change, and acceptance/rejection/normalization
reconcile the proposed caret correctly. Execute its action and reconcile props
in the same worker input turn. Reflect `TextInputProps` so source definitions
can construct the component; avoid making every component's props require
reflection.

Keep clipboard/IME requests and responses on their existing host protocol.
Events are application callbacks, not a replacement for that protocol.

## Language integration and diagnostics

- Parse `@event` attributes and resolve them against the registered component's
  event table, rather than hard-coding `on_activate`.
- Carry inherited container slice scopes through resolution. Resolve qualified
  and unqualified action calls before checking their payload arguments; retain
  spans for scope declarations and both parts of qualified action names.
- Resolve bare event parameters and `event` paths with the declared payload
  descriptor, ahead of enclosing loop locals. Check field paths, expression
  types, named request fields and request adapters before registration.
- Preserve separate spans for argument names and value expressions. Report
  unknown/missing fields with the available request field names and duplicate
  arguments with a secondary label on their first occurrence.
- Keep spans on callback attributes, action calls and payload accesses through
  all stages. Unknown event diagnostics list available component events.
- Reject event access in ordinary props, wrong payload fields/types and calls in
  pure expressions. Report duplicate callbacks, unknown slice/action names and
  unqualified calls outside a slice scope at their source spans.
- Lower Rust and language bindings to one runtime event representation. Move
  general callback execution out of the language-specific runtime; retain
  language compilation and source diagnostics in `ui_language`.
- Include expressions captured by callbacks in i18n traversal. Evaluation uses
  the source instance's presentation settings and the existing translation
  table.

## Migration and scope

Replace `activation`, `change` and `compiled_activation` storage with event
bindings. Keep `with_activation` and `with_change` as thin migration helpers if
they simplify current examples, but do not retain independent runtime paths.
Update hit eligibility, focus defaults and binding compatibility checks to use
event capabilities rather than merely the presence of an activation closure.
Publish bindings atomically with successful geometry preparation; failed frames
must not install callbacks or input metadata from a partial tree.

Migrate button/checkbox activation and controlled text changes. Add a small
custom component emitting a payload from mouse input to prove that event support
is not restricted to built-in components. Convert the showcase text-input pane
to `.pixui` as the integration example. Full showcase migration is out of scope.

No general event bus, ancestor propagation, callback return values, timers,
async callbacks or painter-generated events in this iteration.

## Alternatives

- Separate event constants or an associated event enum: explicit and easy to
  inspect, but require components to construct payload values rather than call
  natural methods. Choose an associated trait and generated proxy for
  ergonomics.
- `&mut self` event methods: allow a plain queue without interior mutability,
  but make shared access through component contexts less convenient. Choose
  `&self` with short worker-local `RefCell` borrows.
- `on_` callback attributes: conflate callbacks with ordinary prop identifiers.
  Choose `@event` to keep the syntactic distinction and unrestricted prop names.
- Global `actions.` lookup or UI-wide slice defaults: flatten ownership or
  require extra configuration. Container inheritance expresses scope where the
  subtree is declared, with explicit qualification for cross-slice calls.
- More special cases such as `on_change`: small initially, but duplicates
  binding lifetimes, language handling and routing for each new component.
- Untyped string-to-closure maps: easy to add, but lose registration checks,
  payload reflection and useful compiler diagnostics. Use names only at lookup
  boundaries and typed/indexed events thereafter.
- Callbacks stored in props: obscures ownership and requires props constructors
  to carry executable values. Keep declarative behavior bindings separate.
- Evaluate everything against live state at emission: avoids snapshots but can
  address a different loop/match tree than the published input target.
- DOM-style propagation: useful eventually, but needs container behavior,
  cancellation and ordering rules beyond the current leaf-component model.

## Implementation checklist

- [x] Finalize and document the associated event trait, payload contracts and
  callback snapshot semantics.
- [x] Implement `#[component_events]`, generated descriptors/payloads/proxies,
  the empty interface and the associated-type adapter.
- [x] Add registration validation, index lookup and safe typed emission
      adapters.
- [x] Introduce shared prepared callback programs with capture and payload
      stages.
- [x] Add worker event routing/emission, ordered action execution and published
  binding compatibility/invalidation.
- [x] Migrate core activation and text editing without duplicate runtime paths.
- [x] Add generalized `.pixui` callback resolution, checking, lowering and
      spans.
- [x] Add `@event` syntax, inherited container slice scopes, slice-qualified
  environment exports and checked action resolution; migrate existing sources.
- [x] Add named action arguments, implicit event parameter locals, explicit
  `event.field` access and shadowing diagnostics; migrate positional callbacks.
- [x] Reflect text-input props and migrate the showcase input pane.
- [x] Demonstrate a custom event component with a mouse-generated payload.
- [x] Update component, input, text-input and language documentation, plus
  `docs/Architecture.md`; record significant decisions following DR-000.
- [x] Run the final `./n check` successfully.
- [x] Complete native validation in both showcase windows and languages.
- [x] Move this plan to `completed` after implementation and validation.

## Verification

- Registration rejects duplicate names, mismatched payloads, foreign IDs and
  out-of-range indices; typed Rust bindings reject incorrect payloads.
- Macro compile-fail tests cover unsupported trait signatures and parameters;
  runtime tests cover shared proxy references, multiple methods and empty
  traits.
- Headless tests exercise empty payloads, owned strings, custom reflected
  fields, unbound events, repeated emission and event/action type errors.
- Language tests cover mixed event/captured arguments, lazy expressions, nested
  loops, stale entity generations, unknown callbacks, source spans and i18n.
- Named-argument tests cover reordered fields, missing/unknown/duplicate names,
  positional-call rejection, zero-field requests and forbidden injected fields.
  Test equivalent bare/qualified payload access, nested field access, loop-local
  shadowing, reserved parameter names and parameter scope isolation from props
  and adjacent callbacks. Reject explicit parameter-binding syntax.
- Scope tests cover nested overrides, sibling isolation, loops/match/fragment
  inheritance, explicit cross-slice calls, missing defaults, duplicate action
  names in different slices, invalid handle ownership and unexported actions.
  Parser tests distinguish `@event` attributes from props and preserve spans.
- Worker tests verify pointer press/release targeting, keyboard activation,
  consumption, ordered emissions, failure behavior, target removal, clipping,
  stale revisions and two windows with shared state but independent geometry.
- Retain controlled editing tests for rejected/normalized content, selection,
  copy/paste, IME and read-only inputs. Add equivalent source-defined tests.
- Confirm failed preparation leaves previous published callbacks intact and
  incompatible preparations replace them even when bounds remain unchanged.
- Manually validate button/checkbox keyboard and mouse behavior, custom click
  payloads, and all showcase text-input modes in both windows/languages.

## Open questions and recommended defaults

- Should common pointer/key payloads be available to custom components? Provide
  reusable reflected payload structs, while letting components define semantic
  payloads. Avoid requiring every event to carry window coordinates/modifiers.
- Should hooks see mutable per-instance state? Yes, for component-specific
  behavior, with shared focus/hover/editing accessed through the existing worker
  policy. Never provide mutable application access during hooks.
- Is click-without-activation needed on standard buttons? Keep `activate` as
  their primary event; demonstrate `click` on a custom component first. Add
  standard click events only with a clear rule preventing two accidental action
  calls.
- Should mouse capture or event bubbling be included? Defer general versions;
  preserve existing text-selection capture and add only press identity tracking
  necessary for correct clicks.

## Implementation notes

- Implementation started after committing this plan as `ef6c133`.
- Runtime event bindings, capture adapters and dispatch programs live in
  `events`; general expression programs live in `expression`. Language
  compilation retains spans and source origins without owning a separate
  activation runtime.
- Rust event bindings use generated typed keys such as
  `TextInputEventsBindings::CHANGE`. `NoEvents` is the empty associated
  interface. Input hooks declare `INPUT_INTEREST` explicitly; its default is no
  raw input.
- Proxy implementations own a clone of their local sink, sharing only its
  `Rc<RefCell<_>>` queue. This permits a static trait-object associated type
  without requiring a borrowed proxy to implement a static interface lifetime.
- Captures currently preserve owned data roots and defer field chains/operators.
  Custom captured roots require explicit `value_type` adapters. Reflected event
  parameter descriptors are exported automatically; merely inspecting their
  fields does not require Clone or Sync.
- Source callbacks require named arguments, including during migration. Reserved
  parameter roots also include `layouts` and `styles` to avoid namespace
  ambiguity.
- Constant `focus` attributes were added to preserve the showcase's deliberately
  focusable button without an activation binding during its source conversion.
- The showcase uses `ui/text.pixui` for controlled inputs and demonstrates a
  custom mouse payload in `ui/language.pixui`. Full showcase conversion remains
  separate work because image, dynamic style and container layout gaps remain.
- Rust requires proc-macro entry points at the crate root (verified with the
  pinned compiler). `engine-macros/lib.rs` contains only the necessary thin
  entry point and its module declaration; expansion logic lives in a named
  module.
- Input batches can contain an ordered request-building error. Earlier requests
  execute before that error is reported; later emissions are discarded. Action
  failures likewise stop the remaining batch without rolling back earlier
  changes.
- Native validation passed in both showcase windows, confirmed by the user on
  2026-10-13. The plan is moved to `completed` after the final repository check.

### Verification results

`./n check` passes all seven tasks, including workspace nextest and doctests.
Coverage includes shared proxy emission, macro signature failures, named
argument ordering and diagnostics, nested/inherited slice scopes, parameter
shadowing, non-Clone/non-Sync custom payload inspection, captured loop fields,
ordered action and request-building failures, typed Rust bindings, stale
revisions, matching pointer gestures, shared hover despite input consumption,
controlled editing and both example applications. Native showcase verification
was requested and is still pending.
