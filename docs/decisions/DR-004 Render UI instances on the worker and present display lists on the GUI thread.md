# DR-004: Render UI instances on the worker and present display lists on the GUI thread

- Status: Accepted
- Date: 2026-10-04

## Decision

Register reusable named `UiDefinition`s on the application worker. Create an
independent `UiInstance` for each window, containing `LiveState`,
`PresentationSettings`, layout and hit regions, interaction state, and a render
revision. Multiple instances share application data while keeping their UI
state separate.

Generate complete owned `RenderOutput`s containing an ordered `DisplayList` on
the worker. Publish through a mailbox holding at most one pending output per
instance; newer outputs replace older pending ones without waiting for the GUI.
Present them on the GUI main thread, which owns native windows and graphics
resources. Send semantic input and presentation changes through the existing
bounded application command queue.

Initially reject discrete input unless it matches the current rendering
revision and geometry. Any dispatched action invalidates all instances,
including actions that return an error after potentially mutating state. Use
winit and softbuffer for the first native host and a shared bitmap font for
measurement and CPU drawing.

## Rationale

Worker-side rendering can borrow application collections directly during live
part traversal. It keeps component state, action execution, layout, and hit
testing under one owner without locks around application data.

Separating definitions from instances enables simultaneous themes, locales,
window sizes, and independent interaction state. Owned display lists separate
stateful rendering decisions from native presentation and permit deterministic
headless tests. Replacing pending output bounds the queue and prevents a slow
GUI from blocking application replies.

## Context

The engine already owns slices and typed collections on a worker and supports
live-part traversal with persistent physical state. The todo example originally
printed that tree. We need actual windows, input-driven updates, and multiple
renderings of the same UI with different presentation settings.

The existing walker accepts mutable templates and loop state follows positions.
The renderer therefore walks a private template copy and keeps only the
instance's persistent state. Action bindings capture opaque item references,
not sequence positions. Native resources stay on the platform event thread.

## Consequences

- Each instance has independent layout, component state, focus, hover,
  scrolling, and settings. Application actions update all instances
  conservatively.
- Native redraws can reuse the retained output without another worker walk.
- Rendering shares the application worker and delays subsequent commands.
  Batches of at most 32 commands limit rendering starvation; inspection acts as
  a barrier for preceding UI work.
- Each instance retains its displayed geometry and only one pending output.
  The mailbox bound limits output count, not display-list size.
- The native host polls outputs at most every 16 ms. This avoids dedicated
  forwarding threads at the cost of periodic wakeups and presentation latency.
- Native callbacks enqueue without blocking. A finite retry queue coalesces
  adjacent motion/settings messages and preserves discrete commands. Exhausting
  it reports a host error rather than silently dropping an action.
- Exact revision matching can reject clicks while newer output awaits
  presentation. Geometry history could improve this later with explicit rules.
  Superseded pointer motion is discarded without diagnostics.
- Positional loop state cannot safely preserve focus across item reordering;
  content invalidation clears focus and hover. Stable keyed reconciliation is
  follow-up work.
- Rendering failures retain the last successful output and geometry; error
  details are inspectable on the instance. Partial component initialization is
  not rolled back.
- Private template copies, full display lists, conservative invalidation, and
  CPU drawing trade throughput for a small initial implementation.
- Fixed-cell Latin text demonstrates shared measurement and German labels;
  complex shaping, bidi, and general font selection require further work.
- Linux desktop behavior is the first validation target. Other platform support
  from the backend requires separate verification.

## Considered alternatives

### Render while borrowing application state on the GUI thread

Rejected because it requires shared synchronization or state copies and couples
native drawing to application ownership. The existing worker can resolve live
parts and actions sequentially with direct borrows.

### Share one physical UI state across all windows

Rejected because dimensions, hit regions, focus, scrolling, and presentation
settings differ between windows. A reusable definition can share the template
without sharing those values.

### Queue every render output in a blocking FIFO

Rejected because obsolete frames consume memory and a slow presenter could
block the worker while the GUI waits for an application reply. Complete outputs
allow safe replacement of pending older output.

### Send component trees or reflected application values to the GUI

Rejected because the GUI only needs painting commands. Reflected borrows cannot
outlive worker-side application access, and duplicating layout or hit testing
would introduce disagreeing state owners.

### Introduce incremental rendering and dependency tracking immediately

Rejected because full outputs and conservative invalidation exercise the
architecture with fewer contracts. Measurements can establish when incremental
work justifies its complexity.

### Start with GPU rendering and a complete text/layout framework

Rejected for the first implementation because the small todo UI can be drawn
with CPU primitives and shared fixed-cell metrics. More capable text shaping,
layout, and graphics remain possible behind the display-list boundary.
