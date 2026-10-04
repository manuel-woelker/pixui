# DR-005: Register typed components and independent painters

- Status: Accepted
- Date: 2026-10-04

## Decision

Use application-owned registries for typed components and independently selected
painters. A component declares props and `Default` state. A typed handle binds a
live part to a component and a props resolver. Optional typed updates run before
read-only painting. Painters emit owned local drawing commands through a context
with dimensions, presentation settings, and focus/hover information.

Initially use one painter per component per application and fixed-height rows.
Resolve activation behavior independently from appearance.

## Rationale

Typed APIs catch incompatible props and painters during compilation. Checked
internal erasure permits heterogeneous trees and registries without requiring
reflection or exposing unchecked casts. Independent painters let applications
customize existing controls while preserving actions and the renderer.

`Default` provides predictable initialization without factories at every node.
Read-only paint inputs keep component updates visible in the lifecycle. Fixed
rows provide useful dimensions while postponing a general layout design.

## Context

The renderer previously matched a fixed label/button/checkbox enum. Extending
that enum required central renderer changes. Applications need custom components
and different appearances. The worker owns physical UI state and sends owned
display lists to the GUI thread; this thread boundary remains in place.

## Consequences

- New components and painters require no renderer branches.
- Registration validates ownership, duplicates, and painter availability,
  including components in empty loops.
- State is independent per physical node and retained across props changes;
  changing registered component identity resets it.
- Props/state need `Send`, but no `Clone`, `Sync`, or reflection implementation.
- Erased adapters, owned props, and local command buffers allocate during
  render.
- Loops retain positional state, and constant-height rows clip oversized
  content.
- Rendering failures preserve published output but do not undo state updates.
- Multiple painter sets within one application and general layout are deferred.

## Considered alternatives

### Extend the fixed widget enum

Rejected because every new control would still require central drawing changes
and applications could not independently select its appearance.

### Combine component and painter implementations

Rejected because behavior and state would become coupled to one appearance,
requiring replacement components to customize drawing.

### Use reflected dynamic props and state throughout

Rejected because painting already knows the concrete component type. Typed
associated types provide direct field access and compile-time checking with
smaller API requirements.

### Let painters mutate state or dispatch actions

Rejected because rendering would gain hidden interaction effects and repeated
paints could change behavior. Explicit updates and activation bindings make the
lifecycle easier to inspect.

### Introduce layout and multiple painter sets now

Rejected because these require sizing and selection policies beyond the current
need. Constant dimensions and application-scoped selection suffice initially.
