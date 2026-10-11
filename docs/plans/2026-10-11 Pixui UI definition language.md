# Pixui UI definition language

Status: implemented; automated repository checks passed and manual showcase
validation pending. The plan remains active until manual verification passes.

## Goal

Define reusable `UiDefinition` templates in `.pixui` files using React-like
element composition, named properties, and expressions. Compile to the existing
live-part model and preserve source locations throughout lexing, parsing, name
resolution, type checking, and lowering. Keep application behavior in Rust.

## Proposed syntax

```text
ui Todos domain 'todos' {
    <Column gap=8 padding=16>
        <Label text="Todos" />
        <Button label="Add todo" on_activate=actions.add_todo() />
        {for todo in collections.todos =>
            <Row gap=8>
                <Checkbox label="Completed" checked=todo.done
                          on_activate=actions.mark_done(todo) />
                <Label text=todo.title />
            </Row>
        }
        {match entities.hide_done;
            true => <Label text="Completed items hidden" />,
            false => <Label text="Showing all items" />,
        }
    </Column>
}
```

Names and action signatures in this example are illustrative host registrations.
The compiler checks them rather than assuming these actions exist.

- One named definition per file initially; its name is independent of its path.
  The optional translation domain defaults to the definition name.
- Paired and self-closing elements; component names resolve through explicit
  registration. `Row`, `Column`, and `Grid` are reserved container constructs.
- Properties accept expressions directly after `=`, without braces. No implicit
  text children:
  use `Label text="..."` so whitespace is unambiguous and components keep their
  existing leaf semantics. Multiple children form a composite where needed.
- Expressions include strings, booleans, contextual numeric literals, field
  access, parentheses, `!`, equality, and short-circuit `&&`/`||` with
  documented precedence. Support named loop variables and nested access such as
  `todo.details.title`; an inner loop must not erase access to outer variables.
- `collections` and `entities` are explicit host-provided namespaces scoped to
  the definition's binding environment. Bind named entities and collections from
  selected slices; duplicate exported names are errors, not implicit precedence.
- `for` accepts a sequence and lowers to `ForLoopPart`. Collection loops retain
  arena identity; general sequences retain current positional identity
  initially.
- `match` starts with boolean/scalar literal candidates and `_`, matching
  current `MatchPart` capabilities. Reject duplicate candidates and unreachable
  arms; require a fallback when exhaustiveness cannot be established. No
  destructuring.
- Double-quoted string templates are translatable by default:
  `"Hello {todo.title}"`. Single-quoted templates are untranslated:
  `'Hello {todo.title}'`. Both parse interpolation expressions using the same
  restricted expression grammar and support `{{`/`}}` for literal braces. Each
  interpolation has its own source span. Lower double-quoted templates to
  discoverable i18n declarations and single-quoted templates to source-only
  interpolation. Evaluate each argument once; preserve the existing scalar
  formatting rules.
- Derive stable placeholder names from simple paths (`todo.title` becomes
  `todo_title`); give other expressions deterministic occurrence names. Reject
  name collisions between different expressions rather than silently reusing an
  argument. Repeated identical paths share a placeholder. Export normalized
  named templates through the existing catalog mechanism. Changes to generated
  names can require catalog updates; document this explicitly.
- Structural strings such as definition domains and string match patterns must
  use single quotes without interpolation. Translation must not change lookup
  names or match identity. Literal string action payloads use single quotes too;
  double quotes deliberately request translated content even in an action.
- Event attributes are reserved bindings, not ordinary props. An action call is
  a deferred request recipe; evaluating props must never dispatch an action.
  Register action adapters that capture checked opaque entity references and
  owned values. Inject collection arguments through existing dispatch behavior.
- Start with `//` line comments and documented string escapes; no imports,
  functions, arbitrary method calls, mutation, JSX spread, JavaScript, hooks,
  custom component declarations, or source hot reload in the first version.

## Delimiters and parsing unambiguity

Use one brace pair for each child control-flow construct:

```text
{for item in collections.items => <Label text=item.title />}
{match entities.enabled;
    true => <Label text="Enabled" />,
    false => <Label text="Disabled" />,
}
<Label text="Hello {todo.title}" />
<Checkbox checked=!todo.done enabled=entities.enabled && !todo.locked />
<Button text='Debug' on_activate=actions.debug(todo) />
```

Proposed grammar sketch:

```text
for_part   := "{" "for" identifier "in" expression "=>" part "}"
match_part := "{" "match" expression ";" (pattern "=>" part ",")+ "}"
attribute  := identifier "=" expression
```

The `=>` ends the loop sequence expression; `;` ends the match scrutinee.
Neither belongs to the expression grammar. These separators avoid a second
brace pair without guessing where an expression stops and an arm/body begins.
A loop body or match arm is one part; use a container or `<>...</>` fragment for
multiple siblings. Nested controls are valid parts and consume their own braces.
Commas between match arms are required, including the final arm, initially.

Attributes need no braces under the initial expression grammar: a Pratt parser
consumes a complete expression, and the next `identifier =` starts a new
attribute. Assignment and adjacent-expression application are not expressions;
function arguments require parentheses and commas. Require whitespace/comments
between attributes. `>`, `/>`, and the next attribute name cannot be consumed as
operators or operands after a complete expression. Equality is `==`, not `=`.
Strings and interpolation use lexical modes so markup delimiters inside strings
are never treated as tag terminators; no lexer-wide greedy `/>` rule.

This is unambiguous for the proposed operators (`!`, equality, `&&`, `||`),
paths, calls, and literals. Bare `<`/`>` comparisons would conflict with markup
boundaries: if added later, require parentheses for them in attribute values,
e.g. `enabled=(count > 0)`. Record literals and division need similarly explicit
rules before introduction. No claim of compatibility with arbitrary Rust/JS
expressions is made. Verify these conclusions with parser fixtures during
implementation; this plan does not yet provide an executable parser.

## Compiler boundaries and public API

Provide a dedicated `crates/engine/src/ui_language/` module. It is independent
of GUI backends and painters; it uses engine metadata only at binding/lowering.
Expose stage APIs for testing and tooling alongside a convenience compilation
entry point accepting source text, filename, and an explicit binding
environment. Return a compiled definition plus its retained source map, or
structured diagnostics. Do not register definitions or mutate application state
while compiling; the caller installs a successfully compiled definition
afterward.

Pipeline:

1. Register an immutable source snapshot and lex into located tokens.
2. Parse into a syntax AST retaining names and complete source locations.
3. Resolve component, collection, entity, action, property, and local names into
   an indexed resolved representation. Establish lexical scopes for loops.
4. Type-check into a typed representation; resolve fields to descriptor indices
   and validate props, event arguments, operators, loops, and match candidates.
5. Lower typed nodes to `LivePart`, expression-backed bindings, and
   `UiDefinition`. Preserve original locations for registration and runtime
   errors.

Keep unresolved, resolved, and typed representations distinct so downstream
stages cannot accidentally accept unchecked names. Share node IDs and origins
where useful; avoid building a generic compiler framework.

## Source spans and diagnostics

- `SourceId` identifies an immutable source snapshot. `Span` contains that ID
  and a half-open UTF-8 byte range. Validate bounds and character boundaries;
  empty spans represent EOF and missing-token insertion locations.
- Every AST node has a span, including definitions, elements, properties,
  identifiers, expressions, loop bindings, match arms/patterns, and action
  arguments. Tokens and nested name/path segments also carry spans. A node's
  full span includes its delimiters; use narrower spans for individual errors.
- Every resolved/typed node retains its original span. Synthesized lowering
  nodes carry an explicit source origin pointing to the construct that produced
  them; never invent offsets or discard locations after parsing.
- Source storage owns filename and text, with cached line starts. Format line
  and column at presentation time, with a documented Unicode/tab policy and CRLF
  support. Old diagnostics retain their original snapshot after source edits.
- Shared `Diagnostic` includes stage, stable error code, severity, message,
  primary labeled span, optional secondary labels, and notes/help. All stages
  use it, including invalid characters/escapes and unexpected EOF. Duplicate
  declarations label both the duplicate and the first declaration.
- Separate diagnostic data from terminal formatting. Provide filename, source
  excerpt, and underline output; leave machine-readable/LSP adapters for later.
- Recover at safe statement, attribute, child, or arm boundaries to report
  independent errors. Bound diagnostics and parser nesting/input sizes;
  malformed input must make progress and must not panic. Never lower a tree
  containing errors; suppress cascading unknown-type errors after failed
  resolution.
- Retain origins on compiled bindings/expressions or in an owned source map
  referenced by the compiled definition. Attach them to registration/evaluation
  errors, including props construction, stale references, and i18n validation.
  Existing Rust-built templates may have no source origin and keep their errors.

## Integration gaps and proposed solutions

### Component props and bindings

`Component::Props` currently needs only `Send`; `ComponentRegistry` has type IDs
but no field schema or constructor. Add opt-in language registration for
components whose props implement `Reflect`, using their `TypeDescriptor` field
metadata and indexed constructor. Keep ordinary Rust registration available for
non-reflected props. Register aliases explicitly; never infer language names
from Rust type names.

Resolve named attributes to field indices once; evaluate and assemble owned
values in constructor index order each render. Diagnose unknown/duplicate props,
missing required props, and incompatible types before installation. All fields
are required initially unless a host adapter explicitly supplies defaults; do
not infer defaults from zero values. Literal conversions are checked and
explicit (including numeric overflow); no general implicit stringification or
downcasts. Borrowed reflected fields must be copied into owned props through
registered type adapters where necessary; do not extend lifetimes or require
unsafe code.

Current expression props resolvers and activation factories use function
pointers, which cannot capture compiled programs. Add narrowly scoped owned
compiled-binding implementations of the existing erased binding mechanism and
an owned event-factory path. Keep the current typed Rust convenience APIs.

### Expressions and runtime scopes

Current `ExpressionKind` supports computed callbacks, current-value fields,
collections, entities, and i18n. It cannot directly represent literals, named
outer loop locals, field chains, template interpolation, or operators. Add a
small checked expression program/evaluator with indexed lexical bindings, or
extend the existing evaluator with these operations. Prefer one shared
evaluation path; avoid two subtly different implementations of field access and
i18n.

Scope frames borrow current loop values only for evaluation. Produced props and
event requests own their payloads/references. Preserve existing collection-loop
identity detection during lowering rather than wrapping collection reads in a
callback that hides their identity. I18n declarations remain traversable before
any loop is expanded, including inactive match arms and nested expressions.

### Layout, styles, and events

Use reserved `layout`, `styles`, and event attributes to avoid collisions with
props fields. Initially bind layout and style assignments from constants or
explicit host-exported typed values, using existing validation and painter slot
metadata. Dynamic layout/style expressions need runtime resolver support and
are a separate extension. Define Grid track syntax before its implementation.
Reject children on leaf components and unknown painter style slots.

Support `on_activate` first. Controlled text input's `on_change` requires a
typed event-local `content` value and separate change adapter; include it in a
follow-up only after specifying ownership and request argument rules. Do not
allow arbitrary Rust callbacks or reflected methods to execute from source text.

## Implemented module layout

```text
ui_language/
  mod.rs                    # module declarations only
  compiler.rs               # public pipeline and compiled result
  source/{mod.rs, snapshot.rs, span.rs, origin.rs}
  diagnostic/{mod.rs, report.rs, render.rs}
  lexer/{mod.rs, scan.rs, token.rs}
  ast/{mod.rs, definition.rs, part.rs, expression.rs}
  parser/{mod.rs, parse.rs, expression.rs, tests.rs}
  resolution/{mod.rs, environment.rs, resolver.rs, resolved.rs}
  checking/{mod.rs, checker.rs, expression.rs, typed.rs, conversions.rs}
  lowering/{mod.rs, lower.rs, binding.rs, runtime.rs}
  tests.rs
  README.md
```

Files use distinct names inside their stage modules to avoid redundant module
names. Markup and expression parsing/checking have separate files. Compiler
intermediates retain spans; immutable source snapshots provide the source map.
All module roots contain only declarations.

## Implementation checklist

- [x] Resolve the open syntax/API choices below and document the initial
      grammar.
- [x] Implement source snapshots, spans, structured diagnostics, and rendering.
- [x] Implement lexer and parser with located AST nodes and bounded recovery.
- [x] Add explicit host bindings and opt-in reflected component props metadata.
- [x] Implement name resolution, nested lexical scopes, and indexed field
      lookup.
- [x] Implement type checking, checked literal conversion, and match validation.
- [x] Add expression runtime/scopes and captured props/event binding support.
- [x] Lower components, containers, loops, matches, and declared i18n
      expressions; propagate source origins into registration and runtime
      errors.
- [x] Load a `.pixui` showcase definition with loops, expressions, and an
      action; compare its behavior with the equivalent Rust-built definition.
- [x] Document embedding/loading APIs, supported syntax, ownership, diagnostics,
  and limits. Update architecture documentation for the compiler boundary.
- [x] Run `./n check` after each completed unit and resolve introduced failures.
- [ ] Complete manual showcase validation and move this plan to `completed/`.

## Verification

- Lexer/parser: empty input, escapes, Unicode, CRLF, comments, multiline markup,
  precedence, mismatched tags, duplicate attributes, EOF, recovery, size limits.
  Cover brace-free adjacent attributes, multiline expressions, nested
  single-brace loops/matches, fragments, separators, tag delimiters inside
  templates, default i18n versus single-quoted templates, escaped braces, and
  placeholder collisions. Assert every node/token span slices the intended
  source and remains in bounds.
- Resolution/checking: unknown names with precise labels, duplicate
  registrations, shadowed/nested locals, outer-loop access, non-sequence loops,
  wrong props/action types, missing fields, field chains, overflow, invalid
  operators, match coverage.
- Diagnostics: stage/code/labels across all four requested stages, primary and
  secondary source excerpts, source snapshots after edits, no cascade floods.
- Lowering/runtime: reflected props construction order, dynamic field changes,
  keyed loop reconciliation, branch switching, i18n extraction/fallback,
  deferred actions, stale references, wrong application/registry bindings,
  source-located runtime failures, and no partial installation after compilation
  errors.
- End-to-end tests run headlessly. Existing Rust definition/component APIs and
  render behavior continue to work. Manual showcase checks cover activation,
  translations, and focus across loop/branch changes.

## Implementation choices and refinements

1. JSX-like elements use the agreed single-brace controls and bare attributes.
   Double quotes translate; single quotes interpolate without translation.
   Parser fixtures exercise separators, nesting, fragments and adjacent
   attributes.
2. Host bindings live in an opt-in `Environment`, preserving ordinary component
   registration. Reflected props use sendable constructors in field order.
   All fields are required; reserved layout/styles/event names cannot be props.
3. Shared indexed expression programs extend `ExpressionKind`. Their declared
   children are traversed by i18n registration. Collection expressions remain
   direct collection nodes so loop arena identity is preserved.
4. Runtime lexical scopes retain outer loop inputs. Opaque action refs are
   allowed only for collection-loop variables and exported entities. General
   sequence items cannot become arena refs. Action payloads are owned snapshots;
   collection injections remain omitted from requests.
5. Reflection metadata and ownership adapters are separate: field chains need
   `Reflect`; owned custom values require explicit Clone/Send/Sync adapters.
   Standard Label/Button/Checkbox props opt into sendable reflection. Borrowed
   field chains use safe consuming reflected views, including typed Any fields.
6. Layout uses constant numeric container attributes, explicit exported
   `LayoutStyle` values, and Grid track literals (`Npx`, `Nfr`, `auto`). Styles
   use exported slot assignments. Painter slot/family validation uses existing
   application registration; errors retain the definition source origin.
7. Template placeholders derive from source paths, not numeric runtime indices.
   Identical paths reuse arguments; conflicting normalized names are errors.
   More complex expressions use deterministic occurrence names. Path/source
   edits can change catalog keys and require updated translations.
8. Input is caller-supplied text, with `include_str!` used by the showcase.
   `CompiledDefinition::install` installs a whole definition; `into_template`
   embeds it with the enclosing definition's domain. Compilation is headless;
   installation uses existing transactional registration and source locations.
9. Stage diagnostics are structured; runtime/registration errors retain
   filename/line/column through the existing shared error vocabulary.
   Independent sibling errors are collected with a bounded count. Parser limits
   and malformed source mutations are tested. Diagnostic columns count Unicode
   scalars.
10. `on_change`, imports, source components, dynamic layouts/styles, native
    metadata syntax, and source hot reload remain deferred as planned. Plain
    collection views cannot be assigned to owned props/action fields; read an
    exported owned sequence or its fields instead.

## Verification record

- Source/lexer/parser checks passed, including UTF-8/CRLF, nested quoted
  interpolations, single-brace controls, brace-free attributes and malformed
  input.
- Compiler/runtime checks passed before final hardening: nested scopes and field
  chains, scalar conversion, match validation, default i18n/source templates,
  deferred actions, stale refs, and foreign application indices.
- The showcase includes a UI language pane from
  `examples/showcase/ui/language.pixui`. Worker tests exercise actual clicks,
  shared updates, new loop rows, conditionals and German translations.
- Architecture, root README and showcase documentation reference the compiler.
- Final `./n check` passed all seven tasks, including formatting, compilation,
  Clippy, nextest and documentation tests. Parser tests include bounded
  input/depth, character-deletion mutations, and valid source spans for
  malformed inputs.
- Follow-up tests verify short-circuit evaluation of stale entity reads,
  independent type diagnostics, general-sequence reference rejection, and atomic
  installation failure. Reflection tests cover borrowed field chains, owned-view
  rejection, descriptor mismatches and sequence storage limits.
- Manual validation is pending: select the UI language showcase pane, activate
  its buttons/checkbox, and compare collection rows and translated/source-only
  templates in the English and German windows. The plan stays active until this
  verification succeeds.
