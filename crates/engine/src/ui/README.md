# Worker-side UI rendering

A `UiDefinition` is a named reusable `LivePart` tree. Register it through
`ApplicationHandle::register_ui`. Create independent realizations using
`create_ui(definition_id, presentation_settings)`, which returns a
`UiInstanceId` and an `OutputReceiver`. Each instance retains physical state,
layout geometry, hit regions and action bindings, focus, hover, scroll offset,
and a render revision. Native windows and surfaces belong to `pixui-gui`.

## Components

Attach a `WidgetFactory` using `ComponentPart::with_presentation`. The ordinary
state factory still initializes persistent component state through the walker.
The presentation callback reads `ExpressionContext` and `PresentationSettings`
on every render and returns a `Widget::Label`, `Button`, or `Checkbox`.
Components without presentation remain valid inert nodes.

Buttons and checkboxes contain an `ActionBinding`: a worker-local function that
builds a fresh `ActionCall` on activation. Capture cached `ActionHandle`s and
opaque `ObjectRef<T>` values. Dispatch revalidates collection identity and arena
generation. Avoid capturing positional sequence indexes or using blocking
`ApplicationHandle` methods inside callbacks on that application's worker.

Rendering walks a private copy of the definition template and retains only the
instance's `LiveState`. This protects definitions from the legacy mutable walker
API. Loop state follows positions; content invalidation clears focus and hover
until keyed reconciliation is implemented.

## Outputs and input

The renderer collects widgets, measures fixed-cell text, lays out vertical rows,
and generates a complete `RenderOutput`. Its `DisplayList` contains ordered
opaque-color rectangle, stroke, text, and clipping commands in logical pixels.
The shared font has basic Latin and Latin-extension glyphs; complex shaping,
bidi, kerning, and font fallback are outside the first implementation.

Each instance retains at most one pending output; publication never waits for
the consumer. Use `OutputReceiver::try_recv` in a native event loop, or
`recv_timeout` for headless verification. Dropping the receiver releases the
instance on the next worker rendering pass.

GUI input is an owned `UiCommand` with instance ID and presented revision.
`try_ui_command` returns a pending reply immediately or retains the command in
its full/disconnected error. Native callbacks must retry full-queue input
without blocking; `ui_command` is a blocking convenience for setup and tests.
Pointer movement, activation, scrolling, focus traversal, and focused activation
are semantic events. Raw platform key transitions are not exposed.

The worker rejects stale discrete input and discards superseded pointer motion.
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
