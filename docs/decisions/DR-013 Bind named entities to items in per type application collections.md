# DR-013: Bind named entities to items in per type application collections

- Status: Accepted
- Date: 2026-10-06

## Decision

Let slices bind individual reflected items by name. `ApplicationSlice::bind`
stages an initial value; attachment resolves it into an ObjectRef backed by one
unnamed application collection per concrete type. Existing references can be
shared through `bind_entity`. Registered slices also support application and
handle helpers for binding new values and retrieving typed refs.

Keep explicit collections separate. Entity names have their own slice-local
namespace and are append-only. Use EntityMut parameters for named action
injection and entity expressions for direct reflected reads. Replace the todo
settings singleton with a named hide_done boolean.

## Rationale

Reusing typed arenas and ObjectRef preserves existing identity, generation and
borrowing checks. Several values of the same type can share storage while names
select individual items. Slice-local names organize values without changing
application ownership established in
[DR-012](<DR-012 Own collections in application storage and bind them by index.md>).

Staging permits the short `slice.bind(name, value)` API without requiring
callers to create collections or allocate refs before configuring their slices.
Keeping staging internal avoids another public builder concept. Explicit
EntityMut parameters distinguish named injection from caller-supplied item
references.

## Context

The todo visibility flag previously required a settings struct, a singleton
collection invariant, and a loop to establish its reflected context. More simple
flags would require additional singleton storage or a larger settings struct.
The application already supports homogeneous erased collections and opaque
references to their individual items.

## Consequences

- Ad hoc storage is allocated lazily and shared by exact TypeId within an app.
  Explicit collections of the same type are independent.
- Staged values need Reflect and Send; Sync is not required. Rejected names and
  invalid slice attachments drop their supplied values without inserting them.
- Slice attachment validates all existing references before inserting staged
  values. Ordinary validation failures leave application storage unchanged;
  panics and allocation failures have no rollback guarantee.
- A staged value has no ObjectRef until attachment. Callers retrieve typed refs
  afterward through entity_ref. Pending staging is empty on registered slices.
- Erased bindings share small address metadata through Arc, not the stored
  value. Safe typed downcasts and reflection adapters avoid unchecked arena-key
  casts.
- Removing a slice preserves items. Explicit arena removal invalidates bindings
  and cached expressions through the existing generation check.
- Anonymous collections have empty diagnostic names and cannot be registered as
  named collections through the public registration path.
- Action registration checks mutable entity aliases and overlap with injected
  whole collections. Generated adapters retain their one-mutable-argument limit.
- Bound entities use a separate namespace from collection and action names.
  Rebinding an existing name is rejected, preserving cached type checks.

## Considered alternatives

### Separate singleton storage

Rejected because it would duplicate storage, reference validation and lifecycle
machinery already provided by collection arenas.

### One explicit collection per named value

Rejected because callers would configure and maintain many collections simply
to store independent values of the same type.

### Expose a separate slice builder

Rejected because internal pending values provide the desired staging behavior
without another public construction type. Resolved bindings remain the runtime
representation.

### Interpret every mutable item argument as named injection

Rejected because existing actions use mutable item arguments supplied through
ObjectRef request fields. EntityMut makes the two selection mechanisms explicit.

### Permit binding replacement

Rejected because cached action validation and expressions could silently switch
identity or type. Append-only bindings preserve their meaning.
