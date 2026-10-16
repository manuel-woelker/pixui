# UI definition language

`.pixui` files compile to the same live parts used by Rust-built definitions.
Compilation is headless and explicit: Rust registers components, application
bindings, and actions, then supplies an `Environment` and source text. Source
files cannot execute Rust/JavaScript, mutate data, or discover arbitrary
methods.

## Example

```text
ui Todos domain 'todos' {
    <Column slice='todos' gap=8 padding=16>
        <Label text="Todos" />
        {for todo in collections.todos => <Row gap=8>
            <Label text="Todo: {todo.title}" />
            <Button label="Mark done" @activate=mark_done(todo: todo) />
        </Row>}
        {match entities.hide_done;
            true => <Label text="Completed items hidden" />,
            false => <Label text="Showing all items" />,
        }
    </Column>
}
```

A file contains one named definition with one root part. Use `<>...</>`
fragments for multiple siblings. Components are leaves; `Row`, `Column`, and
`Grid` contain children and retain the engine's default clipping behavior. Names
are ASCII identifiers and case-sensitive. Source text and string contents
support UTF-8. Whitespace between children is ignored; implicit text children
are rejected.

Properties are expressions directly after `=`, with whitespace or comments
between attributes. An expression ends at the next attribute or tag delimiter.
No assignment, juxtaposed calls, or comparison `<`/`>` operators are supported,
so markup boundaries remain unambiguous. Parentheses group expressions; calls
require parentheses and comma-separated arguments. Attribute braces are not
used.

`{for name in expression => part}` has one brace pair. `=>` ends the sequence
expression. `{match expression; pattern => part, ...}` has one brace pair; `;`
ends its scrutinee. Every arm ends with a comma. Patterns are scalar literals or
`_`; booleans must cover both values or have a wildcard, while other types
require a wildcard. Duplicate patterns and arms after a wildcard are rejected.

## Strings and translation

- `"Hello {todo.title}"` is a translated template by default.
- `'Hello {todo.title}'` interpolates without translation.
- `text=todo.title` reads application text without translating its contents.
- `{{` and `}}` produce literal braces. Escapes are `\n`, `\r`, `\t`, `\\`,
  `\'`, and `\"`. A lone literal `}` is an error.
- Interpolations accept the same pure expression grammar, including nested
  string templates. Each argument evaluates once. Scalars use existing
  locale-independent formatting; structs and sequences cannot be interpolated.

Paths generate readable placeholder names: `todo.title` becomes `todo_title`.
Other expressions use occurrence names such as `arg1`. Repeated identical paths
share arguments; collisions from different paths are errors. Source/path changes
can change catalog keys. Structural strings (domains, string match patterns,
Grid tracks) use untranslated literals. Domain literals currently cannot contain
escapes or interpolation.

Messages inherit the definition's translation domain (defaulting to its name).
Installing a definition registers messages in all arms and loop bodies without
running them. Export and catalog fallback use the existing PO/POT system.
Sources in catalogs refer to the `.pixui` filename and line/column. An embedded
compiled template inherits the enclosing Rust definition's domain.

## Host API

```rust
use pixui_engine::{
    application::app::Application,
    components::label::LabelComponent,
    ui_language::{compiler::compile, resolution::environment::Environment},
};

let mut app = Application::default();
let label = app.register_component::<LabelComponent>("label")?;
let mut environment = Environment::new();
environment.component("Label", label)?;
let compiled = compile("example.pixui", "ui Example {<Label text=\"Hello\" />}",
    &environment).map_err(|errors| pixui_base::pixui_error!("{errors}"))?;
// Register a painter before installing or rendering the compiled definition.
let definition = compiled.definition;
assert_eq!(definition.name(), "Example");
# Ok::<(), pixui_base::PixuiError>(())
```

`Environment::collection::<T>(alias, app, key)` and
`entity(alias, app, reference)` validate application ownership, concrete types
and liveness immediately. They export `collections.alias` and `entities.alias`.
`action(app, slice, alias, handle)` exports `slice.alias(...)`, validates slice
ownership and retains the registered request schema. `action_alias` provides an
explicit identifier alias for slice names unsuitable for dotted calls. Namespace
exports reject duplicates; aliases are explicit and unrelated to definition
names.

`component(alias, id)` is opt-in: props must implement `Reflect` with a sendable
field-ordered constructor. `#[pixui_reflect::reflect(send)]` on an inline module
provides this for custom props. Label, Button, Checkbox, TextInput and Image
props already support it. Ordinary Rust component registration and unreflected
component state remain supported. Every props field is required; there are no
implicit defaults. Types must match, except contextual numeric literals and
validated string-to-`ResourcePath` conversion. Attribute order is independent of
constructor order.

Primitive/string ownership adapters are provided. Export custom metadata through
`reflected_type::<T>()` for nested field/sequence access. If a field must become
owned props or an action value, use `value_type::<T>()`, requiring
`Reflect + Clone + Send + Sync`. Merely reading fields and iterating collections
does not impose Clone/Sync on stored items. Component props themselves need not
be Clone/Sync. Field lookup is performed once, then evaluation uses indices.

Constants support booleans and checked signed/unsigned decimal numbers, with
optional fractional digits. Numeric literals use the receiving field's type;
otherwise integers default to `i64` and fractions to `f64`. Exponents and
suffixes are not supported. `!`, `==`, `!=`, `&&`, `||` operate on supported
scalar types; logical operators short-circuit. Equality preserves concrete
numeric types and Rust `PartialEq` behavior. There are no arithmetic or
arbitrary method calls.

## Loops and actions

Loop variables have lexical scopes; inner loops may shadow names while other
outer names remain accessible. Collection loops retain full generational arena
identity. Other reflected sequences retain positional identity.

Bind declared component events with `@name=action(...)`, for example:

```text
<Column slice='todos'>
    <TextInput content=entities.draft @change=set_draft(value: content) />
    <Button label="Add" @activate=add(title: entities.draft) />
</Column>
```

A container's static `slice='name'` scope is inherited by descendants, including
loops, fragments and match arms. Nested containers override it for their
subtree. An explicit `todos.add(...)` call overrides that scope. Unqualified
calls require an enclosing scope; actions are never searched globally. Hosts
export an allowlist of slice/action pairs, so an inherited scope cannot grant
access to other actions.

Arguments are named request fields: `edit(content: content)`. Their order is
independent of constructor order. Unknown, duplicate, missing or positional
arguments are errors; zero-field requests use `action()`. Injected collections
and entities are absent from the request schema and cannot be supplied
explicitly.

Event method parameters are locals inside their callback: `content` and
`event.content` access the same field. Parameters shadow loop locals there;
rename a loop binding if both are needed. There is no explicit parameter binding
syntax. The `event`, `entities`, `collections`, `layouts` and `styles` parameter
names are reserved. Ordinary props cannot access callback parameters. Action
names have separate lookup rules, so a parameter cannot hide a slice in a call
target.

Callbacks capture owned application/loop values during successful preparation;
payload-dependent expressions run only when the component emits the event.
Logical operators remain lazy. Custom captured roots require explicit
`value_type::<T>()` ownership adapters, including for callback field chains.
A bare collection-loop variable or exported entity can satisfy an `ObjectRef<T>`
request field. General sequence elements cannot supply opaque refs. Dispatch
checks generations, application identity and liveness again.

Repeated emissions produce fresh requests from published captures. Source
application reads do not retarget to a newer tree. Compatible visual redraws
retain bindings; content/presentation invalidation prepares fresh ones.
Controlled text inputs display `content` props and propose replacements through
`@change`. Callbacks are optional: unbound text inputs remain selectable and
read-only. See [component events](../events/README.md) for Rust declarations and
routing.

## Layout and styles

Containers accept constant nonnegative logical-pixel `gap`, `padding`, `width`,
`height`, and `grow`. Grid additionally accepts `columns='100px 1fr auto'` and
`rows='auto'`; tracks are whitespace-separated `Npx`, `Nfr`, or `auto`.

Export richer `LayoutStyle` values with `environment.layout(name, layout)` and
use `layout=layouts.name` on components or containers. Export painter slot
assignments with `environment.styles(name, assignments)` and use
`styles=styles.name` on components. Styles and layouts are constants; dynamic
expressions are deferred. Painter slot/family validation runs during
installation using the application's registered painters and styles.

Components accept constant `focus='automatic'`, `'sequential'`, `'direct'` or
`'none'` to preserve focus participation independently of callback bindings.

## Diagnostics and stages

Infrastructure lives in separate `source`, `diagnostic`, `lexer`, `ast`,
`parser`, `resolution`, `checking`, and `lowering` submodules.
`compiler::compile` coordinates these stages; stage functions are available for
tools/tests. Resolved/typed nodes are separate from syntax nodes. They are
compiler intermediates; callers should use the stage functions rather than
fabricate unchecked intermediate trees.

Every syntax node, path identifier, template segment and interpolation has a
half-open UTF-8 byte span in an immutable `Source` snapshot. Tokens, resolved
and typed nodes retain origins. Source snapshots are reference counted; retained
errors do not read an edited file from disk. EOF errors have zero-width spans.

Diagnostics carry stage, code, severity, primary span, optional secondary
labels, and notes. `CompileErrors` owns the snapshot and diagnostics and formats
source excerpts. Columns count Unicode scalars, with tabs treated as one column;
they are logical positions rather than terminal display-cell widths. Sibling
recovery reports independent errors without lowering incomplete trees.
Field-name errors are classified as resolution diagnostics even when descriptor
lookup happens alongside type checking.

Limits are 1 MiB source input, 128 parser/string nesting levels and 32
independent errors. Existing i18n template, output and registration limits also
apply. Exceeding limits is a diagnostic, not a panic. Compiled bindings retain
filename/line/column for runtime errors; failed registration remains
transactional. `CompiledDefinition::install` registers the whole definition;
`into_template` embeds it in another definition.

File loading is caller-owned (`include_str!` or explicit text reads). Imports,
source components, source hot reload, arbitrary code, and native metadata syntax
are deferred. The resource/image/catalog hot-reload mechanism remains separate.

See the [showcase source](../../../../examples/showcase/ui/language.pixui) and
[host bindings](../../../../examples/showcase/src/language_demo.rs).

## Image resource paths

Register the core image component with
`env.component("Image", components.image)` and configure the application's image
loader in Rust. Layouts remain separate:

```text
<Image path='images/pixui-logo.png' layout=layouts.logo />
<Image path=entities.filename layout=layouts.logo />
<Image path='images/{item.filename}.png' layout=layouts.logo />
```

Whenever a prop or action argument expects `ResourcePath`, strings are validated
with `ResourcePath::new`. Static literals are validated during checking; dynamic
strings and templates are validated on the worker. Already typed paths pass
through unchanged. Compilation never checks file existence or decodes pixels.
The standard environment supplies the path ownership adapter, including for
custom reflected props and action requests.

Use single quotes: translated templates, including nested translated
expressions, are rejected for paths. Paths do not enter translation catalogs.
Application string values must contain structural filenames, regardless of
presentation language. There is no reverse conversion or arbitrary
constructor-call syntax.

Dynamic validation and loading errors retain source locations and prevent frame
publication, preserving the last successful output. Correcting the value permits
recovery. Images retain normal snapshot caching and optional resource hot
reload.
