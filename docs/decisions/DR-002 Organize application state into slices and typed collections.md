# DR-002: Organize application state into slices and typed collections

- Status: Accepted
- Date: 2026-10-04

## Decision

Represent application state as named `ApplicationSlice` instances containing
named, homogeneous `Collection` instances and registered actions. Each collection
owns an `Arena<T>` for one concrete Rust type, erased at the collection boundary
so different types can coexist in the same slice. Store items directly, without
per-item `DynamicObject` wrappers or a requirement that items implement `Reflect`.

Use stable slice identities, collection identities, and typed generational keys
to address individual items through `ObjectRef<T>`. Requests carry these owned
references; dispatch resolves them into temporary Rust borrows. Inject mutable
collections into action handlers by argument name, validating their existence
and item type during registration. Multiple collections may hold the same type.

Keep slice names unique within an application and collection names unique within
a slice. Names are immutable after construction. Generated action facades bind
to a named slice once and cache indexed action handles.

Ordinary Rust structs remain the representation for individual domain objects.
The collection model defines the application's storage and addressing boundary;
it does not require every nested field to become another collection.

## Rationale

The engine needs a common state model that can host different applications
without knowing their concrete root struct. Slices provide a discoverable domain
boundary, while collections provide a uniform way to locate typed storage and
validate action dependencies. For example, a `todo` slice contains a `todos`
collection and the actions that operate on it.

Stable item addresses let callers construct requests without borrowing worker
state. This supports queued dispatch from other threads while preserving normal
exclusive Rust borrows inside handlers. Generational keys reject references to
removed or reused slots instead of allowing an old request to target a new item.

Homogeneous arenas retain concrete item layouts and typed access. Erasing one
arena per collection keeps heterogeneous application composition possible without
boxing and dynamically inspecting every item. Reflection remains useful for
request schemas and dynamic consumers, as described in
[DR-001](<DR-001 Use a custom reflection mechanism.md>).

Collection names distinguish storage roles when the same type appears more than
once, such as `todos` and `archived`. Registration checks catch missing or
mistyped bindings before an action can run. Ordinary handler functions and typed
facades keep most application code independent of these runtime lookups.

## Context

The application runs on an internally owned worker thread. Callers communicate
through a bounded channel and should be able to invoke actions without holding
locks or borrowing the application state. An add action needs mutable collection
access supplied by dispatch; a mark-done action needs a mutable item resolved
from an opaque reference in its request.

A plain root struct could store arenas and support this execution model too.
The additional requirement is a shared engine interface for discovering slices,
collections, and actions, and binding requests to storage without writing a
root-specific resolver for each application. Reflection over fields alone does
not define collection identity, stale-item handling, or action injection rules.

This record documents the implemented architecture. It does not establish a
serialization format, database model, authorization boundary, or performance
advantage over direct struct field access.

## Consequences

- Applications can compose domain slices without defining an engine-specific
  root struct or resolver. Collection item types remain ordinary Rust types.
- Requests retain item addresses without retaining borrows. Removal, foreign
  arena keys, and generation mismatches are checked when resolving references.
- State construction and action registration require explicit naming and setup.
  Renaming an injected parameter changes its collection binding; names must be
  kept consistent with collection configuration.
- Some mistakes become runtime errors: unknown slices or collections, incorrect
  item types, stale references, and incompatible action registration. Typed
  handlers and facades reduce these errors but do not eliminate them.
- Lookups currently scan vectors and collection access performs checked
  downcasts. Cached action indices avoid repeated action-name resolution, but
  this organization does not make storage lookup constant time. Add indexing
  only when workloads justify it.
- Collection items must be `'static + Send` so their owning state can move to
  the worker; `Sync` is unnecessary. Borrowed items cannot be stored here.
- Arena identities and generations are finite, and references are process-local.
  Persistence would need separate durable identities and resolution rules.
- Slices group behavior and storage but do not isolate access: an `ObjectRef<T>`
  can identify an item in another slice. References are addresses, not permissions.
- Singleton settings, nested ownership, and cross-collection invariants may fit
  this model less naturally than a root struct. Do not split cohesive domain
  objects merely to make all data independently addressable. Revisit the model
  if actual applications need substantial workarounds for noncollection state.
- This storage choice does not imply transactions, rollback, persistence, or
  reactive change notifications. Those require separate decisions and contracts.

## Considered alternatives

### Store all application state in a plain Rust root struct

Rejected as the engine's common state boundary because discovering storage and
binding actions would require generated or handwritten adapters for each root
type. Direct field access would be simpler and more statically checked for a
single fixed application, and a root struct could still own arenas and use a
worker channel. We accept the collection model's setup and runtime checks for
uniform composition, addressing, and dispatch across applications.

### Reflect a root struct and treat its fields as storage

Rejected because field discovery alone supplies no contract for collection
identity, generational item references, or injection. Adding conventions and
adapters to arbitrary reflected fields would recreate the collection mechanism
with less explicit boundaries. Reflection continues to serve its own dynamic
inspection and invocation needs.

### Use one heterogeneous map or arena of dynamic objects

Rejected because individual items would need erased storage and runtime type
handling. Homogeneous collections let handlers access an `Arena<T>` directly
and keep erasure at the collection boundary. A generic map would also need
additional rules for item identity, removal, and stale references.

### Allow only one collection per item type

Rejected because type identity cannot distinguish roles such as active and
archived todos. Binding by argument name permits multiple collections of the
same type while keeping handler signatures ordinary Rust functions.

### Adopt an entity component system

Rejected for the current requirements because entity composition, component
queries, and scheduling add concepts not needed by the todo-style action model.
Typed arenas already provide stable checked item handles. Reconsider if actual
workloads require entities composed from independently queried components.

### Give each slice a separate worker and independent state ownership

Deferred because resolving references across slices and coordinating actions
would require additional communication and consistency rules. The current
application worker owns all slices and provides sequential execution; slices
are domain organization rather than thread boundaries.
