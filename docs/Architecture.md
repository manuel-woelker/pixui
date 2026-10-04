# Architecture

Pixui is a Rust workspace with an application engine, shared infrastructure, and
a small reflection mechanism. The current implementation provides state storage,
action dispatch, and live-model traversal. A web UI, persistence layer, and
reactive change notifications are not implemented.

## Application runtime

![Application runtime: caller threads send commands through a bounded queue to a worker that owns slices and typed collections; results return through reply channels.](diagrams/architecture.drawio.svg)

The SVG embeds its editable draw.io diagram. Open it with draw.io Desktop
through `./t drawio docs/diagrams/architecture.drawio.svg` to edit it; save with
diagram data embedded so the file remains both viewable and editable.

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

The worker processes actions, registration, and inspection sequentially.
`dispatch` waits for queue capacity and returns a pending reply. `try_dispatch`
returns a full-queue error with the original call for retry. Each command that
expects a result has a reply channel carrying an owned `PixuiResult<T>`.
Inspection returns an owned snapshot; live state borrows cannot escape.

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
updates, leaving unvisited entries potentially unknown. This is separate from
the application dispatch mechanism, with no automatic state-to-view update
pipeline.

See the [reflection documentation](../crates/reflect/README.md) and
[DR-001](<decisions/DR-001 Use a custom reflection mechanism.md>) for supported
operations and limitations.

## Workspace responsibilities

| Package | Responsibility |
| --- | --- |
| `pixui-base` | Typed arenas and keys, shared errors/results, strings, and sendable erased values. |
| `pixui-reflect` | Runtime type descriptors, construction, dynamic objects, and sequence access. |
| `pixui-reflect-macros` | Reflection attributes, action adapters, and generated slice facades. |
| `pixui-engine` | Application state, worker dispatch, action registration, and live-model traversal. |
| `pixui-example-todo` | Runnable example of a todo slice, collection, and typed actions. |

The engine uses base storage and reflection metadata. Generated code connects
ordinary application definitions to those runtime APIs. Keep domain objects
cohesive; collections provide addressable storage without requiring every nested
field to become an independently stored object.
