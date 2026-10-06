# Worker-side UI rendering

A `UiDefinition` is a named reusable `LivePart` tree. Register it through
`ApplicationHandle::register_ui`. Create independent realizations using
`create_ui(definition_id, presentation_settings)`, which returns a
`UiInstanceId` and an `OutputReceiver`. Each instance retains physical state,
layout geometry, hit regions and action bindings, and a render revision. Focus,
hover and requested scrolling live in shared `UiDefinitionState`; input from one
window refreshes all instances of that definition. Native windows and surfaces
belong to `pixui-gui`.

## Components

Register typed components and independent painters before registering a UI.
`ComponentPart::typed(id, resolver)` resolves props on each render and
initializes persistent state through `Default`. `typed_with_update` adds an
explicit update callback before read-only painting. Legacy components without
typed bindings remain valid inert nodes. See the
[component guide](../component_registry/README.md).

Attach an activation factory with `with_activation` to produce an
`ActionBinding`: a worker-local function that builds a fresh `ActionCall` on
activation. Capture cached `ActionHandle`s and opaque `ObjectRef<T>` values.
Dispatch revalidates collection identity and arena generation. Avoid capturing
positional sequence indexes or using blocking `ApplicationHandle` methods inside
callbacks on that application's worker.

Rendering walks a private copy of the definition template and retains only the
instance's `LiveState`. This protects definitions from the legacy mutable walker
API. Loop state follows positions; content invalidation clears focus and hover
until keyed reconciliation is implemented.

## Shared interaction

`UiRegistry::definition(id)?.state()` exposes shared focus, hover and requested
scrolling. Hover hit testing uses all prepared component bounds, not just action
hit regions. The engine supplies `PaintContext::hovered` to every painter;
components without activation can still draw a hover effect. Focus traversal
visits only components with activation. Both refer to component order, including
noninteractive components, so an action hit-region index is not a component
index.

The latest processed pointer input wins across windows; leaving a window clears
shared hover. Interaction does not affect other definitions. Hidden instances
retain pending updates and reflect shared state when shown. Scroll changes
invalidate geometry for all peers; each clamps the shared requested offset to
its viewport without changing the shared value during rendering. Focus and hover
changes retain compatible input revisions while geometry is unchanged.

Component identity currently follows prepared tree positions. Instances sharing
a definition must retain corresponding component order; independent conditional
structures need stable component keys before sharing interaction safely. Content
invalidation clears positional focus and hover. Closing a window preserves
shared state for other or future instances.

## Outputs and input

The renderer prepares typed props and paints constant-height rows, translates
and clips local commands, and generates a complete `RenderOutput`. Every row is
36 logical pixels high; text overflow is clipped. Its `DisplayList` contains
ordered rectangle, stroke, text, image, and clipping commands in logical
pixels. Text references immutable indexed grayscale font atlases prepared in a
worker batch. Embedded Geist supplies Latin glyphs, real metrics, and coverage
antialiasing. Complex shaping, bidi, kerning, and system fallback remain outside
the initial scope. See [text drawing](Text.md) for baseline placement, font
acquisition, cache limits, and DPI behavior.

Each instance retains at most one pending output; publication never waits for
the consumer. Use `OutputReceiver::try_recv` in a native event loop, or
`recv_timeout` for headless verification. Dropping the receiver releases the
instance on the next worker rendering pass.

GUI input is an owned `UiCommand` with instance ID and presented revision.
`try_ui_command` returns a pending reply immediately or retains the command in
its full/disconnected error. Native callbacks must retry full-queue input
without blocking; `ui_command` is a blocking convenience for setup and tests.
`UiInput` carries logical pointer coordinates, mouse button presses/releases,
wheel deltas with their original line/pixel units, all keyboard transitions
(including repeat, text, physical identity and location), modifiers, native
window focus and IME events. Named keys and physical codes use the documented
winit names; native unidentified key codes remain opaque strings. No winit types
or native window resources enter the engine.

The application's default input policy activates on left-button release,
traverses focus with Tab (backwards with Shift+Tab), activates focus with Enter
or Space, and converts each vertical wheel line to forty logical pixels. Key
releases, repeats and synthetic keyboard transitions do not activate or toggle
shortcuts. Other buttons/keys, text, IME and native focus transitions are
forwarded but currently have no default behavior. There is no component event
propagation, pointer capture or text editing yet.

The worker rejects geometry-dependent stale input and discards superseded
pointer motion. Unhandled raw events and F11 do not need compatible geometry;
leaving a window clears hover even while geometry is stale.
Visual animation redraws retain compatible presented revisions and existing
bindings while geometry is unchanged; content changes end that compatibility.
It rejects activation while content, viewport, or scrolling changes make old
geometry stale. Focus and hover changes within a batch can still use the same
geometry until a new revision is published. Closing an instance invalidates its
ID and disconnects output delivery; late commands return errors.

## Scheduling and failures

Any dispatched action invalidates all instances, including a handler that
returns an error after changing data. Presentation and interaction changes
invalidate their own instance. Rendering happens after at most 32 queued
commands. Inspection first flushes rendering for preceding commands; ordinary
action replies acknowledge execution and do not wait for presentation.

Rendering failure retains the last good output, revision, and layout and records
`UiInstance::last_error`. Component initialization already performed is not
rolled back. The next invalidation retries. Inspect instance state through
`ApplicationHandle::inspect`; borrows and callback bindings never leave the
worker. All thread-boundary values are owned and `Send`.

See [the architecture](../../../../docs/Architecture.md) and
[the todo GUI](../../../../examples/todo/src/gui_ui.rs) for the complete flow.

Image commands share immutable RGB snapshots through an indexed resource table.
Painters can request a future visual redraw without invalidating focus or hover.
See [dynamic images](Images.md) for drawing and scheduling contracts.

Images, fonts, and glyph atlases use
[shared typed resource handles and tables](Resources.md).

## Performance diagnostics

F11 toggles a worker-owned overlay independently for the source instance. The
worker adds the box and prepared monospace text to the normal display list;
native hosts only present it and report `UiCommand::FramePresented` with the
successful painting revision, renderer timings and observation timestamp.

While visible, the worker wakes every 250 ms and rebuilds diagnostics from a
cached, unadorned application output. Component preparation and painting are
skipped for these refreshes. Hidden windows pause them, and disabling
diagnostics removes their deadline. Output `revision` advances while
`paint_revision` stays unchanged; consumers must preserve painter redraw
deadlines and animation acknowledgements for these diagnostic-only outputs. FPS
counts distinct successfully presented painting passes, so diagnostic refreshes
and repeated presentations do not inflate it. Headless clients can toggle
diagnostics and optionally supply presentation feedback without a native host.

See
[the renderer diagnostics guide](../../../gui/src/renderer/README.md#performance-overlay)
for timing and memory interpretation, and
[DR-011](<../../../../docs/decisions/DR-011 Interpret native input and render diagnostics on the application worker.md>)
for the ownership decision.
