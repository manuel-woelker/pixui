# Architecture

Pixui is a Rust workspace with an application engine, shared infrastructure, and
a small reflection mechanism. The current implementation provides state storage,
action dispatch, live-model traversal, and a native GUI with worker-side
component preparation and rendering. A web UI and persistence layer are not
implemented.

## Application runtime

![Application runtime: callers and the GUI thread send commands to the worker; the worker owns application and UI instance state and publishes display lists for native windows.](diagrams/architecture.generated.svg)

The source is [plain draw.io XML](diagrams/architecture.drawio). Edit it with
`./t drawio docs/diagrams/architecture.drawio`. Run `./n watch-diagrams` to
generate the ignored SVG automatically on changes, or `./n render-diagrams`
for a single export pass. A fresh checkout needs an export before this image
is available.

### State organization

`Application` owns an append-only vector of homogeneous collections and named
`ApplicationSlice` instances. Slices group local collection bindings and
registered actions. Slice names are unique within an application; binding and
action names are unique within their slice. Different slices or local names can
reference the same collection. Bindings are append-only and cannot be replaced.

`register_collection` returns an opaque `CollectionIndex` containing a vector
position and collection identity. Lookup is direct and validates identity, so a
foreign application's index cannot retarget a same-position collection. There
is no collection deletion or index reuse; storage lives until application
shutdown, including after its last slice binding is removed. Collection names
are diagnostic defaults rather than globally unique identifiers.

`ApplicationSlice::bind_collection` configures named indices before `add_slice`,
which validates all collection identities. Alternatively, `add_collection`
registers storage and binds its default name to an existing slice atomically.
Action registration goes through the application to check bound collection
names, types and conflicting mutable aliases before modifying the action list.
Generated facades provide `register(&handle, slice_id)` and
`register_in(&mut application, slice_id)` for the two ownership contexts.

Name-to-index maps resolve slice names and slice-local collection names. Slice
identity maps to its current vector position; removal and reordering refresh
slice maps. `CollectionKey` aliases `CollectionIndex`.
`collection_key(slice_name, collection_name)` resolves bindings once;
`resolve_collection(index)` accesses the application store. Expressions carry
these indices and remain valid after slice removal or reordering.

A `Collection` owns an `Arena<T>` erased through `Any`. Items remain ordinary
Rust values and need no reflection wrapper or `Reflect` implementation. Items
must be `'static + Send`; `Sync` is not required. Multiple collections can
contain the same type.

Slices also hold named entity bindings. `slice.bind(name, initial_value)` stages
a `Reflect + Send` value; `add_slice` validates names and existing references
before inserting staged values into one unnamed collection per concrete type.
These collections use the same append-only store and typed arenas as explicit
collections, but stay separate from them. Multiple named booleans occupy
different slots in the shared boolean arena. Registered slices support
`application.bind(slice_id, name, value)`; `bind_entity` shares existing refs.
Entity, collection and action names have independent namespaces.

`EntityMut<T>` action parameters resolve slice-local entity names and are
omitted from requests. `&mut T` parameters still resolve caller-supplied item
references. Registration checks types, liveness and overlapping mutable
bindings; dispatch revalidates generations. Entity expressions read checked
reflected refs directly. Removing a slice preserves its items; explicit item
deletion leaves stale bindings that fail rather than retarget. See
[DR-013](<decisions/DR-013 Bind named entities to items in per type application collections.md>).

An eight-byte `Key<T>` packs a slot index, arena identity, and generation
counter. An `ObjectRef<T>` combines a typed key with a collection index so a
request can identify an item without borrowing state. Resolution checks the
collection identity, concrete type and arena generation. Removing a slice
invalidates its action calls while preserving collections and item references.
These addresses are process-local and are not authorization tokens or persistent
identifiers.

See
[DR-002](<decisions/DR-002 Organize application state into slices and typed collections.md>)
for the rationale and tradeoffs compared with a plain application struct, and
[DR-012](<decisions/DR-012 Own collections in application storage and bind them by index.md>)
for the separation between collection storage and slice organization.

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
reached, and loops maintain one independent body state per element. Registered
components create owned, sendable payloads through their state type's `Default`
implementation. Legacy unregistered nodes retain state factories for
non-rendering visitors. Walks retain state by position, resize child lists, and
drop removed state. Stable item identity across reordering is not implemented.
Visitors receive initialized state and can update it; template edits are
reconciled before descent. Traversal preserves child and element order; errors
stop it without rolling back earlier updates, leaving unvisited entries
potentially unknown. The GUI renderer uses this walker to build display lists
after application commands.

See the [reflection documentation](../crates/reflect/README.md) and
[DR-001](<decisions/DR-001 Use a custom reflection mechanism.md>) for supported
operations and limitations.

## Native UI rendering

### Definitions, instances, and windows

`Application` owns a `UiRegistry` of named `UiDefinition`s and stable
`UiDefinitionId`s. A definition contains a reusable `LivePart` template and
shared `UiDefinitionState` for focus, hover and requested scrolling. Each
`UiInstance`, identified by `UiInstanceId`, holds the worker-side state for one
window or headless target:

- Persistent `LiveState`, initialized with `PartState::Unknown`.
- `PresentationSettings`: light/dark theme, application-defined locale, logical
  viewport size, and scale factor.
- `LayoutState`: component bounds, clipped hit regions and action bindings,
  and content height.
- Derived scroll offset for its viewport, rendering revision, and the last
  rendering error.

Multiple instances of the same definition share application collections and
interaction state. Component state, presentation settings and geometry remain
per instance. The engine detects hover for all prepared components and passes
shared hover to every painter through `PaintContext::hovered`, including
components without actions. Hover and focus currently identify prepared
component positions; instances must retain corresponding component order. Shared
scrolling is clamped per viewport for drawing without modifying the shared
requested offset. The initial native host maps one instance to one window.
Native winit windows and renderer-owned graphics surfaces belong to the process
main thread. `ApplicationHandle` still contains only a cheaply clonable command
sender.

`register_ui` and `create_ui` transfer definitions and settings to the worker;
creation returns the instance ID and its output receiver. Registration rejects
empty or duplicate names. IDs are process-local and never reused. Closing an
instance invalidates its ID; dropping its output consumer also releases it on
the next worker rendering pass.

### Components, painters, and display lists

Each application owns a `ComponentRegistry` and a `PainterRegistry`. Register
component types, then their painters, before registering UI definitions.
`Component` associates owned props with persistent `Default` state. A copyable
`ComponentId<C>` contains registry identity and an append-only index; duplicate
names/types and foreign handles are rejected. UI validation checks all component
bindings, including empty loop bodies, for a registered painter.

`ComponentPart::typed` binds an ID to a props resolver. Every render resolves
props once per physical component; `typed_with_update` additionally runs a typed
state update before painting. The walker initializes state at first reach and
preserves it until the registered identity changes. Props and state need `Send`,
without `Clone`, `Sync`, or reflection. Checked internal downcasts connect the
heterogeneous tree to typed callbacks.

`Painter<C>` implementations are registered independently and receive a
`PaintContext` with immutable props/state, dimensions, settings, focus, and
hover. Commands use local coordinates and enter one shared builder through the
context. All components resolve props and update state before painting; state is
reborrowed in physical tree order without evaluating expressions again. One
painter per component per application allows different applications to customize
the same component type. Explicit standard component and painter helpers remain
independent. Activation factories create action bindings separately, so changing
appearance preserves behavior.

The worker paints fixed vertical rows with 36 logical pixels of height, 8 pixels
of spacing, and 16 pixels of outer padding. Available width is clamped to zero.
It clamps scrolling to the content height, translates commands into row
positions, and clips each component and the viewport. Composites and loops add
no boxes. There is no measurement or general layout API; oversized content is
clipped. The renderer walks a private template copy and retains independent
physical state.

See the [component API guide](../crates/engine/src/component_registry/README.md)
and
[DR-005](<decisions/DR-005 Register typed components and independent painters.md>)
for the lifecycle, constraints, and alternatives.

`RenderOutput` contains the instance ID, monotonic `RenderRevision`, and an
owned `DisplayList`. Commands paint in order: `FillRect`, `StrokeRect`,
`DrawText`, `DrawImage`, `PushClip`, and `PopClip`. Rectangles, glyph metrics,
and stroke widths use logical pixels. Nested clips intersect and must be
balanced. Colors are opaque sRGB. Commands contain no reflected application
borrows, callbacks, or native handles. `DrawImage` uses an index into the
display list's image table. Each immutable RGB snapshot shares pixels through
`Arc`; the builder deduplicates snapshot identity and discards its reverse
lookup on completion. Color-key transparency skips matching source pixels, and
images use nearest-neighbor sampling. Retained outputs keep their exact image
versions without upload history.

Image and font tables share `ResourceTable<T>` and `ResourceTableBuilder<T>`.
Images, fonts, and glyph atlases use `Resource<T>` handles with allocation
identity and immutable Arc-backed ownership. Typed `ResourceIndex<T>` aliases
prevent mixing image and font indices, while remaining local to a frame/table.
See [shared render resources](../crates/engine/src/ui/Resources.md) for lifetime
and validation contracts.

### Text resources

`DrawText` contains a string, first baseline, color, and index into
`DisplayList::fonts`. The worker collects characters across all painters, then
prepares missing glyphs with fontdue and packs them with etagere before
publication. Each immutable `Font` resource contains real logical metrics,
a character map, and one grayscale coverage atlas rasterized for its DPI.
Unchanged frames/windows reuse snapshots; growth preserves retained outputs.
The worker owns a bounded LRU cache and commits each font snapshot atomically.

Tool-tool downloads pinned static Geist Regular TTF; its bytes are embedded at
build time. The GUI performs only prepared character lookup and coverage
blending, with baseline bearings, fractional advances, explicit newlines, and
physical pixel snapping. Retained frames safely resample during DPI transitions.
Coverage blends in encoded RGB. Initial scope is Latin character-based text,
without shaping, bidi, kerning, automatic wrapping, or system font discovery.
See [the text contract](../crates/engine/src/ui/Text.md) and
[DR-007](<decisions/DR-007 Prepare character atlases on the worker.md>).

### Images and animation

Runtime code constructs an `Image` from dimensions, RGB pixels, and an optional
transparent color. Replacement creates a new snapshot, leaving old output
unchanged. Painters insert images directly into the shared builder with local
rectangles; commands carry table indices. This avoids copying unchanged source
pixels. The software backend retains full-frame CPU drawing costs; the
femtovg backend uploads snapshots once per cache residency and draws on the GPU.

Painters call `request_animation_frame()` on frames needing a successor. The
worker combines these requests into `RenderOutput::animating`; the GUI owns
pacing for each window. After successful presentation, the host requests a
native drawing opportunity and then sends one `AnimationFrame` command. A
request ID acknowledged in completed output keeps at most one animation request
outstanding, including queueing and rendering. Unrelated action output cannot
release it early. Slow workers skip achievable frames rather than accumulating
ticks. Native redraw callbacks, presentation notification, FIFO and a monitor
refresh-rate cap provide pacing; exact vsync timestamps are not exposed.

Delayed `request_redraw_after()` requests remain available for occasional
updates. Continuous animation takes precedence. Suspended, occluded and
zero-size windows pause scheduling; restoration rearms it. Visual redraws retain
focus, hover, scrolling and existing bindings while geometry matches. Compatible
older presented revisions remain usable for clicks; content changes still reject
stale input. See [animation scheduling](../crates/engine/src/ui/Images.md).

The todo GUI includes an `OrbitingComets` custom component. Its painter
generates a fresh small transparent image each frame, using the master
`PaintContext::timestamp_us` rather than a paint counter or a node-local clock.
The application clock supplies a `u64` microsecond timeline sampled once per
render and shared by every painter. Instances share its epoch; an optional
`PresentationSettings::timestamp_us` override freezes or seeks an instance
for controlled drawing and tests. `None` resumes automatic time.

See the [image guide](../crates/engine/src/ui/Images.md) and
[DR-006](<decisions/DR-006 Share immutable images in indexed display lists.md>).

### Updates and communication

Any dispatched action invalidates all instances, including an action that
returns an error after mutating data. Content invalidation clears focus and
hover in each definition because loop reconciliation is positional. Settings
changes invalidate one instance; shared interaction changes invalidate every
instance of that definition. The worker renders dirty instances
after batches of at most 32 commands. Inspection flushes preceding rendering
work before reading instance state. Action replies acknowledge execution;
they do not acknowledge native presentation.

Each instance has a latest-output mailbox with capacity one. Publication never
waits: newer output replaces pending older output. Publication and mailbox
disconnection wake the GUI through a callback attached by its consumer. The host
coalesces those notifications into event-loop wakeups, reads the latest output,
and retains it for native redraws without another worker traversal. Idle windows
wait without output polling. A finite retry timer remains while GUI commands or
replies are pending. Output count is bounded; command payload sizes are not.

Native callbacks submit `UiCommand`s using `try_ui_command`. The host retains
full-queue commands for retry in a bounded queue of 256 pending commands and
replies. Adjacent pointer-motion or presentation updates can be coalesced;
discrete commands keep their order. Overflow or worker disconnection reports a
host error rather than silently losing an activation.

The host forwards low-level `UiInput`: mouse buttons and wheel units, keyboard
presses/releases/repeats with logical and physical keys, modifiers, text, IME
and native focus events. The application worker interprets activation,
Tab/Shift+Tab navigation, scrolling and F11. Other raw inputs currently have no
default effect; component event propagation and text editing are not
implemented.

Geometry-dependent input identifies its instance and the revision actually
painted by the GUI. The worker performs hit testing against its corresponding
geometry and builds an owned action call from the target binding. Revision
matching rejects stale geometry-dependent events; visual-only redraws permit
older revisions in the same compatible geometry sequence. Superseded pointer
motion is discarded. Pending content, settings, or scroll changes also
invalidate old geometry. Hover or focus painting does not invalidate geometry
within a command batch. Item bindings capture checked opaque references, so a
deleted item cannot silently resolve to a replacement arena slot.

The worker also renders the per-window F11 performance overlay, including its
monospace atlas, into the complete display list. The host reports successful
presentation timestamps and renderer timings through `FramePresented`, and adds
no diagnostic drawing. A worker timer refreshes visible diagnostics every 250 ms
from a cached application display list without component preparation or
painting. Hidden windows skip these refreshes. Diagnostic-only outputs advance
the output revision while retaining `paint_revision`; repeated presentations do
not increase application FPS or restart painter deadlines. Cached application
commands add worker memory, while image and font payloads remain shared.
See
[DR-011](<decisions/DR-011 Interpret native input and render diagnostics on the application worker.md>)
for the ownership rationale.

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
| `pixui-gui` | Native event loop, windows, input submission, and pluggable display-list execution. |
| `pixui-example-todo` | Text and two-window native examples of a todo slice, collection, and typed actions. |

The engine uses base storage and reflection metadata. Generated code connects
ordinary application definitions to those runtime APIs. Keep domain objects
cohesive; collections provide addressable storage without requiring every nested
field to become an independently stored object.

## Conditional live parts

`MatchPart` evaluates an expression once and selects the first typed equality
candidate, with an optional final wildcard. Only that subtree is prepared,
painted, and assigned hit regions. No matching candidate renders nothing.
Selection preserves the surrounding loop value and application context.

`MatchState` retains one active child. Changing or losing selection drops the
previous subtree; returning initializes fresh component state through `Default`.
Candidate identity is positional, like current composites and loops.
See [DR-008](<decisions/DR-008 Retain only the active match subtree.md>) for the
lifecycle rationale. All
candidate templates are validated at registration, including inactive branches.

The todo example stages its named `hide_done` boolean with `slice.bind`. Its
entity expression drives matching directly. Actions mutate it through
`EntityMut<bool>` on the worker and invalidate both windows;
presentation settings remain per-window. Filtering hides rows without removing
stored todos. The native GUI is now the example's default executable.

## GUI renderer plugins

The native host creates a `Renderer` per window through a `RendererFactory`.
SoftwareRenderer wraps the deterministic CPU painter and softbuffer.
FemtovgRenderer uses wgpu, sharing its device/queue across compatible windows
while retaining per-window surfaces and bounded texture caches. Auto prefers GPU
and reports initialization fallback; explicit backend selection is available.
Only successful presentation advances input revisions. Skipped presentation
retries on a bounded schedule; suspend/resume manages native surfaces.

Both backends render worker-generated glyph atlases and existing DrawText
commands. Femtovg performs no font layout or glyph rasterization. Texture caches
use weak snapshot identities and a 64 MiB LRU budget per window. GPU resources
stay off the worker queue. See
[renderer documentation](../crates/gui/src/renderer/README.md) and
[DR-009](<decisions/DR-009 Plug GUI renderers into a shared display list contract.md>).

### Hidden UI instances

Native windows report visibility transitions with `UiCommand::Visibility`.
Hidden instances retain dirty state while skipping preparation, state traversal,
painting and resource finalization. Actions still update application state and
visible instances. Showing an instance requests fresh output using all
accumulated changes and the current animation clock. The GUI uses native
lifecycle events plus a 250 ms check of available visibility and minimization
state. On Wayland, a requested drawing opportunity withheld for 500 ms also
pauses the worker; returning drawing events or explicit restoration resume it.
This is an inference from compositor activity, not an exact visibility query.
Unknown platform visibility is otherwise treated as visible; loss of focus never
hides an instance. Headless instances start visible and can explicitly use the
same visibility command.

## Resource filesystems and images

Engine resource filesystems open owned `Read + Send` readers for validated
relative `ResourcePath` filenames backed by `PixuiString`. Directory sources
have explicit roots and reject paths or resolved symlinks escaping their root.
Layered sources search in priority order and fall back only when a resource is
absent. Opening, reading or decoding failures remain errors from the selected
source.

`ImageLoader` is a cheaply cloneable handle with private `Arc<Inner>` ownership.
It reads a bounded encoded buffer and decodes PNG/JPEG into existing immutable
`Image` snapshots, enforcing pixel and decode buffer limits. Images store either
RGB with an optional color key or straight-alpha RGBA. Both software and femtovg
renderers support the formats; display lists retain snapshots through the same
resource indices and GPU identity cache used for generated images. Font coverage
atlases retain their existing single-channel representation.

The standard core `ImageComponent` takes a relative resource path as its prop.
Configure `Application::set_image_loader` through the handle before rendering.
`Component::prepare` resolves the path into `ImageState` on the worker, after
props resolution and before binding updates or painting. The standard
`ImagePainter` draws that snapshot with centered, aspect-preserving scaling.
Custom painters reuse the prepared image without doing resource I/O.

An application-owned weak path cache shares live snapshots across components and
windows without retaining unused pixel buffers. Changing props resolves the new
path; replacing the loader clears lookup and invalidates existing UIs. Loading
errors preserve previous state and published output. Direct `ImageLoader::load`
still returns a fresh snapshot. Without runtime hot reload, cache misses load
synchronously and can block preparation; painters and native UI threads do no
I/O. Embedded readers can be added without changing decoding. Future network
fetches need background scheduling rather than blocking application dispatch or
treating pending requests as absence.

The todo image component displays `assets/images/pixui-logo.png`, preserving
transparency and aspect ratio. Its `--assets` option installs an override
directory above the default assets source. See
[resource filesystem APIs](../crates/engine/src/resources/README.md) and
[image snapshots](../crates/engine/src/ui/Images.md).

## Native window properties

UI definitions may register a window-properties resolver that derives a title
and optional icon resource path from application state and presentation
settings. The worker resolves icons through the existing image service and
compares each instance's last published properties. Only changed titles or icon
snapshots cross to the GUI as `WindowCommand::SetTitle` or
`WindowCommand::SetIcon`.

A separate coalescing mailbox retains the latest command for each property,
independent of frame replacement. `OutputReceiver::window_commands()` exposes
it; the existing event-loop waker covers both subscriptions. Native metadata is
resolved and applied while hidden without painting. Animation-only and
diagnostic refreshes skip metadata evaluation. Errors retain previous properties
and are reported separately from render errors. Native icon display remains
platform specific. Winit 0.31 supports Wayland icons when the compositor
implements `xdg_toplevel_icon_v1`; macOS requires application-icon integration
beyond the window-icon setter.

The todo example derives localized open-task counts for its window titles and
shares the loaded logo snapshot between its image component and window icon.
See [window properties](../crates/engine/src/ui/Window%20properties.md) for API,
coalescing, lifecycle and native support details.

## Internationalization

UI definitions declare translatable expressions in component bindings and native
window properties. Each definition has one translation domain. Registration
visits all abstract branches and loop bodies, merges identical domain/source/
context keys, and resolves application-local message indices transactionally.
Index zero remains unresolved; indices are never persisted in catalogs.

The worker owns a translation registry with dense indexed templates per
language. Each instance's presentation selects a language index; loop expression
contexts retain that selection. Evaluation interpolates named subexpressions
into an owned string, falling back to source text for missing entries. Rendering
clients receive ordinary text commands and need no catalog or language lookup.

PO/POT import and export are explicit adapters over neutral catalog records.
Domain/language installation is atomic, supports late definitions, and currently
invalidates all UIs and native metadata. Hidden windows update their titles
without painting; retained render outputs remain independent of catalog changes.

See [internationalization](../crates/engine/src/i18n/README.md) and
[DR-014](<decisions/DR-014 Register declared translations by definition domain.md>)
for APIs, identity rules, workflow, and limits.

## Optional resource hot reload

A caller-owned `ResourceReloadSession` enables native directory watching at
runtime; it is disabled by default and requires no Cargo feature. Directory
and layered sources expose native roots without changing ordinary resource
lookup. Images subscribe by requested logical path; catalog targets explicitly
bind a path to a domain, language, and format adapter.

Both use one pipeline: native event hints, per-target debounce, bounded
background reads, image decoding or catalog parsing, then prepared updates
through the existing application command queue. Content hashes suppress
unchanged data; session identity, configuration checks, and revisions discard
obsolete updates. The worker replaces image snapshots directly and
transactionally installs catalogs, invalidating UIs and native metadata. Hidden
windows skip painting.

Only one prepared/sent payload is outstanding. Watched image paths retain their
latest successful snapshots for the session; normal image caching stays weak.
Old outputs keep old images/text independently. Errors and complete deletion
preserve last-good values; directory layers retain their normal fallback rules.
The external guard cancels and joins the loader without waiting for queue
capacity. It must not be owned by the application worker, since the service
holds an application sender.

See [hot reload](../crates/engine/src/resources/reload/README.md) and
[DR-015](<decisions/DR-015 Reload resources through a shared background pipeline.md>)
for setup, lifecycle, and limits.
