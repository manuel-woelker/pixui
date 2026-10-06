# DR-012: Own collections in application storage and bind them by index

- Status: Accepted
- Date: 2026-10-06

## Decision

Store collections in an append-only application-level vector. Give each a stable
opaque `CollectionIndex` with a position and identity check. Slices map local
names to indices and group registered actions. Multiple bindings may reference
the same collection. Retain storage until application shutdown, even after all
referencing slices have been removed.

Address items through collection indices and typed generational arena keys.
Collection expressions and item references are independent of slice lifetime.
Action calls still identify their target slice and fail when it is removed.

Validate action names, collection existence, types and conflicting mutable
aliases through the application. Generated facades register through the worker
handle or directly on the application owner thread.

This supersedes collection ownership and item addressing in
[DR-002](<DR-002 Organize application state into slices and typed collections.md>).

## Rationale

Separating storage from organization makes collection identity independent of
where data is exposed. Slices can share a collection without copying it, while
retaining local argument-name binding for action injection. Direct indexed
resolution removes slice lookup and collection identity scans from item access.
Identity checks prevent foreign indices from accidentally targeting a local
collection at the same vector position.

Append-only storage is simple and avoids index reuse, generations at the
collection level, reference counting and implicit cleanup rules. Existing arena
keys continue to protect individual item access.

## Context

Collections previously belonged to slices. Collection keys combined a slice
identity and local index; item references included the slice and collection
identity. Removing a slice therefore made its collections inaccessible. Slices
could not expose shared storage through independently named bindings.

Action registration previously validated collections on a standalone slice.
The application now owns the information needed for that validation. Handler
macros currently allow at most one mutable argument, but manual descriptors can
advertise multiple collection bindings and require alias checks.

## Consequences

- Collections, expressions and item references survive slice removal. Calls to
  removed slices fail. Names can be reused without retargeting old indices.
- Application registration validates slice indices before attaching a slice.
  Action batch validation is atomic and rejects mutable aliases.
- Slices no longer carry their collection data when removed or transferred.
  Foreign collection bindings are rejected when adding a slice to another app.
- Unbound collections retain their memory until shutdown. Collection deletion
  would require an explicit lifetime policy and generational handles.
- Collection indices use the existing process-wide collection identity derived
  from the originally allocated arena ID. They inherit its finite identity space
  and are process-local, not persistent identifiers or authorization tokens.
- Collection names serve as diagnostics and convenience defaults. Binding names
  belong to slices; global collection names need not be unique.
- Generated facade registration now requires an application and registered
  slice identity. Existing callers must migrate their initialization sequence.

## Considered alternatives

### Keep collections inside slices

Rejected because storage lifetime and identity would remain coupled to slice
organization, and sharing would require additional indirection or wrappers.

### Use unchecked vector positions

Rejected because a foreign index could silently resolve to another application's
collection at the same position. A small identity check keeps direct lookup
safe.

### Automatically delete collections when the last binding disappears

Rejected because expressions and item references can outlive slice bindings.
Tracking those references and defining deletion behavior adds lifecycle
machinery without a current requirement.

### Use a generational arena for collections immediately

Rejected for now because collection deletion is not supported. Append-only
indices plus identity validation meet current requirements with fewer concepts.

### Keep validating action bindings entirely on the slice

Rejected because doing so would require copying collection type metadata into
bindings or giving standalone slices access to application storage. Application
registration provides one authoritative validation point.
