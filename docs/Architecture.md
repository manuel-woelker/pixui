# Architecture

Pixui is a Rust workspace with an application engine, shared infrastructure, and
a small reflection mechanism. The current implementation provides state storage,
action dispatch, live-model traversal, and a native GUI with worker-side layout
and rendering. A web UI and persistence layer are not implemented.

## Application runtime

![Application runtime: callers and the GUI thread send commands to the worker; the worker owns application and UI instance state and publishes display lists for native windows.](diagrams/architecture.generated.svg)

The source is [plain draw.io XML](diagrams/architecture.drawio). Edit it with
`./t drawio docs/diagrams/architecture.drawio`. Run `./n watch-diagrams` to
generate the ignored SVG automatically on changes, or `./n render-diagrams`
for a single export pass. A fresh checkout needs an export before this image
is available.

### State organization

`Application` owns named `ApplicationSlice` instances. Each slice groups named
collections and registered actions. Slice names are unique within an
application; collection and action names are unique within their slice. Names
are immutable, and slice identities remain stable when slices are reordered.

Name-to-index maps resolve slice names and collection names. Slice identity also
maps to its current vector position; removal and reordering refresh the slice
maps. Lookups use hash maps, while collection access by index uses the vector.

`CollectionKey` combines a stable slice identity with an append-only collection
index. `Application::collection_key(slice_name, collection_name)` resolves names
once, and `resolve_collection(key)` borrows the target collection. The handle
provides name resolution through one worker round trip. Collection expressions
carry these keys. Keys survive reordering and additions, but fail after their
slice is removed, including if another slice later reuses its name.

A `Collection` holds one concrete item type in an `Arena<T>`. The arena is
erased through `Any` at the collection boundary, allowing different collection
types in one slice. Individual items remain ordinary Rust values and need no
reflection wrapper or `Reflect` implementation. Items must be `'static + Send`;
`Sync` is not required. Multiple collections can contain the same type.

An eight-byte `Key<T>` packs a slot index, arena identity, and generation
counter. An `ObjectRef<T>` combines a typed key with a slice and collection
identity so a request can identify an item without borrowing state. Resolution
checks the target's identity, type, and generation. These addresses are
process-local and are not authorization tokens or persistent identifiers.

See
[DR-002](<decisions/DR-002 Organize application state into slices and typed collections.md>)
for the rationale and tradeoffs compared with a plain application struct.

### Ownership and communication

`Application::new()` starts a worker immediately and returns an
`ApplicationHandle`. The handle contains only a cheaply clonable sender;
application state lives on the worker. Crossbeam's bounded channel is used as
multiple producers and one consumer. The default capacity is 128 commands;
`with_capacity` allows another bound, including zero for rendezvous.

The worker processes actions, UI events, registration, and inspection
sequentially. `dispatch` waits for queue capacity and returns a pending reply.
`try_dispatch` returns a full-queue error with the original call for retry. Each
command that expects a result has a reply channel carrying an owned
`PixuiResult<T>`. Inspection returns an owned snapshot; live state borrows
cannot escape.

Dropping a reply does not cancel the command. Dropping the last sender allows
accepted commands to drain before normal worker exit, but does not wait for
cleanup. An unwinding panic invalidates state; the worker drops later commands
so their waiting callers receive errors. Ordinary handler errors leave the
worker running and do not roll back mutations.

Blocking handle calls from the same worker can deadlock, including calls inside
inspection callbacks. Long operations delay all later commands. Queue capacity
limits pending command count, not total memory or execution latency.

See
[DR-003](<decisions/DR-003 Own application state on a worker thread with a bounded MPSC queue.md>)
for ownership, lifecycle, and the alternatives to queued dispatch.

### Actions and typed facades

Actions are ordinary synchronous functions. `#[action]` generates an owned
request schema, an `ActionDescriptor`, and a dispatch adapter:

- `&mut Arena<T>` parameters are injected from a collection matching the
  argument name. Registration validates the collection's existence and item
  type.
- `&mut T` parameters become `ObjectRef<T>` request fields. Dispatch resolves
  them into temporary exclusive borrows.
- Owned parameters become owned request fields.

The current adapter supports at most one mutable parameter. Handlers can also be
called directly with normal Rust arguments.

`#[slice_actions(slice = "todo", facade = TodoActions)]` discovers actions in an
inline module and generates registration and a cloneable typed facade. Binding
resolves the slice and verifies exact action descriptors in one worker round
trip. The facade caches `ActionHandle` values containing slice and action
indices plus static metadata. It constructs requests locally and dispatches by
index.

Facade methods wait for typed results and propagate dispatch and handler errors.
Owned `String` arguments accept `impl Into<String>`. Callers that need pending
results or nonblocking admission can use the lower-level handle API.

See the [action API documentation](../crates/engine/src/application/Actions.md)
and the [todo example](../examples/todo/src/todo.rs) for concrete usage.

## Reflection and live models

`pixui-reflect` supplies a non-generic `TypeDescriptor` with field, method, and
constructor metadata. Generated descriptors use static `OnceLock` storage.
Consumers can resolve names once and cache indices for repeated operations. The
`#[reflect]` module attribute discovers ordinary structs and inherent methods
without a second manually maintained member list.

`DynamicObject` represents owned values or shared and mutable references, with
borrow lifetimes preserved for reference results. Sequence descriptors and views
support vector and slice element access. Dynamic wrappers serve local reflected
access; dispatch instead carries owned `Any + Send` values. Generated action
request schemas use reflection for inspection and construction from ordered
fields, while typed facade calls construct their requests directly.

The engine's live model is a `LivePart` tree containing composites, components,
and loops. Each loop has an expression selecting an application collection or a
reflected field in the current context. Its visitor walk retains application
access and uses a sequence element as the current value inside each loop.
`LiveState` retains a physical `PartState` tree: unknown entries initialize when
reached, and loops maintain one independent body state per element. Components
create owned, sendable payloads through their state factories. Walks retain
state by position, resize child lists, and drop removed state. Stable item
identity across reordering is not implemented. Visitors receive initialized
state and can update it; template edits are reconciled before descent. Traversal
preserves child and element order; errors stop it without rolling back earlier
updates, leaving unvisited entries potentially unknown. The GUI renderer uses
this walker to build display lists after application commands.

See the [reflection documentation](../crates/reflect/README.md) and
[DR-001](<decisions/DR-001 Use a custom reflection mechanism.md>) for supported
operations and limitations.

## Native UI rendering

### Definitions, instances, and windows

`Application` owns a `UiRegistry` of named `UiDefinition`s and stable
`UiDefinitionId`s. A definition contains a reusable `LivePart` template. Each
`UiInstance`, identified by `UiInstanceId`, holds the worker-side state for one
window or headless target:

- Persistent `LiveState`, initialized with `PartState::Unknown`.
- `PresentationSettings`: light/dark theme, application-defined locale, logical
  viewport size, and scale factor.
- `LayoutState`: component bounds, clipped hit regions and action bindings,
  and content height.
- Focus, hover, scroll offset, rendering revision, and the last rendering error.

Multiple instances of the same definition share application collections but
retain independent UI state. The initial native host maps one instance to one
window. Native winit windows and softbuffer surfaces belong to the process main
thread. `ApplicationHandle` still contains only a cheaply clonable command
sender.

`register_ui` and `create_ui` transfer definitions and settings to the worker;
creation returns the instance ID and its output receiver. Registration rejects
empty or duplicate names. IDs are process-local and never reused. Closing an
instance invalidates its ID; dropping its output consumer also releases it on
the next worker rendering pass.

### Layout and display lists

Components can attach a presentation callback that reads the current expression
context and instance settings and returns a `Widget`: label, button, or
checkbox. The callback runs on every walk. The renderer walks a private copy of
the shared template to isolate the legacy walker's mutable-template interface.
Persistent physical state remains in the instance.

The worker collects widget properties, measures text, and lays out a vertical
stack before painting. Composites and loops group traversal without adding
layout boxes. Content wraps by fixed character cells and scroll offsets are
clamped to the measured content height.

`RenderOutput` contains the instance ID, monotonic `RenderRevision`, and an
owned `DisplayList`. Commands paint in order: `FillRect`, `StrokeRect`,
`DrawText`, `PushClip`, and `PopClip`. Rectangles, glyph sizes, and stroke
widths use logical pixels. Nested clips intersect and must be balanced. Colors
are opaque sRGB. Commands contain no reflected application borrows, callbacks,
or native handles.

Both threads use the same embedded 8-by-8 bitmap glyphs and fixed-cell metrics.
The painter applies the native scale factor and produces softbuffer-compatible
pixels. Basic Latin and Latin extensions support the English/German example;
unknown characters use a fallback glyph. Complex shaping, bidi, font selection,
and a general localization system are not implemented.

### Updates and communication

Any dispatched action invalidates all instances, including an action that
returns an error after mutating data. Content invalidation clears focus and
hover because loop reconciliation is positional. Settings and interaction
changes invalidate only their instance. The worker renders dirty instances
after batches of at most 32 commands. Inspection flushes preceding rendering
work before reading instance state. Action replies acknowledge execution;
they do not acknowledge native presentation.

Each instance has a latest-output mailbox with capacity one. Publication never
waits: newer output replaces pending older output. The GUI host polls at a
16 ms interval and retains the latest output for native redraws without another
worker traversal. Output count is bounded; command payload sizes are not.

Native callbacks submit `UiCommand`s using `try_ui_command`. The host retains
full-queue commands for retry in a bounded queue of 256 pending commands and
replies. Adjacent pointer-motion or presentation updates can be coalesced;
discrete commands keep their order. Overflow or worker disconnection reports a
host error rather than silently losing an activation.

Input identifies its instance and the revision actually painted by the GUI.
The worker performs hit testing against its corresponding geometry and builds
an owned action call from the target binding. Exact revision matching rejects
stale discrete events; superseded pointer motion is discarded. Pending content,
settings, or scroll changes also invalidate old
geometry. Hover or focus painting does not invalidate geometry within a command
batch. Item bindings capture checked opaque references, so a deleted item cannot
silently resolve to a replacement arena slot.

A rendering failure publishes no partial output. The previous successful
output and geometry remain; details are inspectable through
`UiInstance::last_error`. Physical-state initialization performed before the
failure is not rolled back. A later invalidation retries rendering. Native
surface or event-loop failures are returned from the GUI host.

See
[DR-004](<decisions/DR-004 Render UI instances on the worker and present display lists on the GUI thread.md>)
for the decisions and tradeoffs, and the
[todo GUI](../examples/todo/src/gui_ui.rs) for presentation and action bindings.
Run it with `./t cargo run -p pixui-example-todo --bin gui`.

## Workspace responsibilities

| Package | Responsibility |
| --- | --- |
| `pixui-base` | Typed arenas and keys, shared errors/results, strings, and sendable erased values. |
| `pixui-reflect` | Runtime type descriptors, construction, dynamic objects, and sequence access. |
| `pixui-reflect-macros` | Reflection attributes, action adapters, and generated slice facades. |
| `pixui-engine` | Application state, worker dispatch, live-model traversal, UI instances, layout, and display-list generation. |
| `pixui-gui` | Native event loop, windows, input submission, and CPU display-list painting. |
| `pixui-example-todo` | Text and two-window native examples of a todo slice, collection, and typed actions. |

The engine uses base storage and reflection metadata. Generated code connects
ordinary application definitions to those runtime APIs. Keep domain objects
cohesive; collections provide addressable storage without requiring every nested
field to become an independently stored object.
