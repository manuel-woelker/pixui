# GUI rendering plan

Status: proposed. Start by agreeing on the concepts below; this plan does not
select a GUI backend or introduce runtime code.

## Goal

Walk live parts on the application worker to produce owned draw commands. Send
them to the GUI thread for presentation. Send GUI events back to the worker,
handle them there, and render updated frames. Support multiple UI definitions
and multiple independently configured instances of each definition.

## Proposed names and responsibilities

| Concept | Responsibility | Owner |
| --- | --- | --- |
| `UiDefinition` | Named, reusable live-part tree describing a UI | Application worker |
| `UiInstance` | Worker-side UI state for one window: persistent `LiveState`, presentation settings, layout, hit regions, interaction state, and render revision | Application worker |
| `PresentationSettings` | Theme, locale, logical viewport size, and scale factor for an instance | Application worker, updated through messages |
| `Window` | Native host, platform events, and graphics resources | GUI thread |
| `RenderOutput` | Owned rendering result containing instance ID, revision, and a `DisplayList` | Produced by worker, consumed by GUI thread |
| `DisplayList` | Ordered sequence of drawing commands | Produced by worker, consumed by GUI thread |

Use `UiDefinitionId` and `UiInstanceId` for stable identities. Window identity
belongs to the GUI host; it maps windows to instance IDs.

An application owns multiple definitions. A definition can have many instances.
Initially each window hosts one instance, and each instance has one window.
Keep window resources separate so headless tests can render instances too.

For example, the `todos` definition can have an English light instance at 800 by
600 and a German dark instance at 400 by 800. Both read the same todo
collection. Their component state, focus, scrolling, and presentation settings
are independent.

Each `UiInstance` owns its layout geometry, including component bounds and hit
regions, alongside focus, hover, and scroll state. If this geometry needs a
separate structure, call it `LayoutState`. The native `Window` owns platform
resources; its corresponding `UiInstance` owns the worker-side UI state.

`Ui` alone is ambiguous between definition and instance. `View` is familiar but
already often means a component or data projection. `Viewport` describes the
visible area rather than the persistent interactive instance. Prefer the
explicit definition/instance vocabulary for now.

## Ownership and rendering flow

1. Register a definition and create an instance with presentation settings.
   Initialize its root state to `PartState::Unknown`.
2. The GUI thread creates a window and reports its viewport and scale factor.
3. The worker walks the definition using the instance's state, application
   collections, and presentation settings. Resolve properties, measure and lay
   out components, then emit draw commands in painting order.
4. Publish a complete owned `RenderOutput`. It contains no borrowed application
   values, reflected objects, or native graphics handles.
5. The GUI thread presents the newest available frame for that instance. A
   native repaint can redraw the retained frame without another worker walk.
6. Input messages identify the instance and the presented frame revision. The
   worker resolves the target, updates interaction state or invokes an action,
   and marks affected instances dirty.
7. Initially any application action conservatively dirties all instances.
   Presentation settings and interaction changes dirty their own instance.
   Render once after processing a bounded batch of pending messages.

The GUI event loop should run on the platform's required thread, usually the
process main thread. The existing application worker remains the sole owner of
application and component state.

## Render output and display commands

`RenderOutput` contains `instance_id: UiInstanceId`,
`revision: RenderRevision`, and `display_list: DisplayList`.
`DisplayList` contains an ordered `Vec<DrawCommand>`.

Start with these command variants:

| Command | Contents |
| --- | --- |
| `FillRect` | Rectangle and color |
| `StrokeRect` | Rectangle, color, and stroke width |
| `DrawText` | Origin, owned text, font ID, font size, and color |
| `PushClip` | Clipping rectangle |
| `PopClip` | Restore the previous clip |

Commands execute in order; later commands paint over earlier ones. Coordinates,
font sizes, and stroke widths use logical pixels. The GUI thread applies the
window's scale factor. Clip commands must be balanced, and nested clips
intersect. Font IDs identify agreed font resources rather than native graphics
handles.

Keep hit regions and event bindings in the `UiInstance`, associated with the
render revision. They do not travel in the `DisplayList`. Retaining geometry for
an older presented revision, or rejecting events for it, requires an explicit
stale-event policy.

`DrawText` is the initial command shape. Shared font selection and measurement
must produce consistent layout and painting; shaped glyph runs may be needed
for accurate international text.

## Contracts to establish

- Definitions are reusable templates. Instance-specific mutations belong in
  instance state. The current walker exposes mutable templates; settle how to
  prevent cross-instance edits before sharing a definition.
- Each `RenderOutput` replaces the entire previous output. Start with simple
  rectangle, text, and clipping commands, plus logical coordinates and explicit
  paint values. Rendering order and clip balancing must be documented and
  tested.
- Layout must precede painting. Text measurement must agree with GUI drawing;
  choose a font and measurement strategy before implementing text layout.
- Keep frame delivery bounded and allow newer frames to replace pending older
  frames per instance. The worker must not wait for GUI presentation: a GUI
  thread waiting for an application reply would otherwise deadlock it.
- Input uses the existing bounded worker queue. Native event callbacks must use
  nonblocking submission, with an explicit overflow policy. Coalesce pointer
  motion and resize messages; preserve clicks, key transitions, and close
  events.
- Store hit regions and event bindings on the worker for the corresponding
  frame revision. Define how stale events are rejected or resolved before
  accepting interaction; never silently retarget an old click to another item.
- Closing a window releases its instance and pending frames. Late messages for
  removed instances must fail safely. Closing one window must leave others live.
- Loop state currently follows sequence positions. Interactive rows need stable
  item identity before reordering can preserve focus or component state safely.

## Implementation steps

Keep `docs/Architecture.md` and its embedded architecture diagram current as
the ownership, thread boundaries, and rendering flow are implemented.

- [ ] Agree on the names, ownership, and initial one-window-per-instance rule.
- [ ] Add definition and instance registration, stable IDs, independent state,
  and presentation settings updates. Resolve the mutable-template contract.
- [ ] Define `RenderOutput`, `DisplayList`, and draw-command types; implement a
      headless recording renderer with deterministic output.
- [ ] Add basic component properties and layout for a vertical todo list,
  rectangles, labels, and checkboxes. Establish text measurement and scaling.
- [ ] Add bounded frame delivery and dirty-instance scheduling to the worker.
- [ ] Select a native window/graphics backend and add a GUI host that presents
  frames on its event thread.
- [ ] Add hit testing, revision-aware input routing, and action invocation.
- [ ] Show the same todo definition in two windows with different presentation
      settings. Updating the shared collection should update both windows.
- [ ] Document public contracts, update architecture documentation and diagram,
  and record the agreed architectural decisions.

## Verification

- [ ] Test independent state for instances sharing one definition, including
  loop growth and removal.
- [ ] Test presentation settings changes, shared-data invalidation, frame
      ordering, pending-frame replacement, and a slow or disconnected GUI
      consumer.
- [ ] Test event routing, stale revisions, deleted loop items, queue saturation,
  and window closure while rendering or handling input.
- [ ] Assert thread-boundary payloads are owned and `Send`; keep native
      resources on the GUI thread.
- [ ] Run the two-window example and check resizing, scaling, redraw after
  exposure, interaction, and independent window closure.
- [ ] Run `./n check` after each implementation unit.

## Open questions

- Does a definition need parameters beyond its presentation settings and
  application bindings, such as a selected document? Add them when there is a
  concrete use.
- Which theme and localization behavior should the first example exercise?
  Locale selection alone does not implement translation or text shaping.
- Which backend and platforms are required for the first working GUI?
- Should component handlers receive semantic events, raw input, or both?
- How should rendering failures be reported while retaining the last good frame?

## Later improvements

Start with full frames and conservative invalidation. Consider dependency-based
invalidation, incremental drawing, and more elaborate layout only after the
working example exposes a concrete need. Stable loop identity is an earlier
correctness concern once interactive state must survive item reordering.
