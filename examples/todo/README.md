# Todo example

`Application::new()` starts the worker internally and returns a cheaply clonable
`ApplicationHandle` containing only a bounded MPSC sender. The example adds a
configured `todo` slice containing a `todos` collection of `TodoItem`s.

Ordinary action functions describe their inputs:

```rust
fn add_todo(todos: &mut Arena<TodoItem>, title: String) -> PixuiResult<Key<TodoItem>>;
fn mark_done(todo: &mut TodoItem);
```

`#[action]` generates reflected request types and dispatch adapters. Doc
comments become descriptions. Registration checks that `todos` exists and
contains `TodoItem`; multiple collections of that type are allowed.

- `add_todo` requests contain only `title`. Dispatch injects the named arena;
  blank titles are rejected before insertion.
- `mark_done` requests contain only an opaque `ObjectRef<TodoItem>`. Dispatch
  resolves it to `&mut TodoItem` and rejects stale or foreign handles.

The generated `TodoActions` facade binds to the named slice and caches its
action handles. Request construction is local; facade methods dispatch and wait
for typed results. Facades can be cloned across caller threads. Application
state stays on its worker without `Rc` or `RefCell`. Dropping all handles closes
the queue and drains accepted commands.

## Native GUI

Run from the repository root:

```sh
./t cargo run -p pixui-example-todo
```

The native GUI is the default binary; the previous headless tree-printing UI
has been removed. Engine walker tests cover traversal independently.

The GUI opens two windows from one `UiDefinition`: English/light at 640 by 480
and German/dark at 420 by 640. Each window has its own worker-side `UiInstance`
with presentation settings, component state, layout and hit regions. Focus,
hover and scrolling are shared by the UI definition. Both instances read the
same todo collection.

Click **Add todo** to append a generated task, or click a checkbox to mark a
task done. Both windows update. Tab moves focus; Enter or Space activates the
focused control. The mouse wheel scrolls overflowing content. Resize windows to
see independent clipping, and close either window without closing the other.

Click **Hide completed** / **Erledigte ausblenden** to filter completed rows in
both windows. The named `hide_done` entity holds the boolean flag. The
zero-argument `toggle_hide_completed` facade method dispatches an action that
receives the entity through `EntityMut<bool>`.
A `MatchPart` chooses the complete list or a loop that matches each todo's
`completed` field. Hidden rows have no layout space or hit target; todos remain
stored. Switching the list branch drops its component state and recreates it
on return. Completing a visible item in filtered mode hides it immediately.

Click **Pause animation** / **Animation pausieren** below the comet animation to
pause both windows. The button changes to **Resume animation** /
**Animation fortsetzen**. Pausing stops animation frame requests and preserves
the last image snapshot on unrelated redraws, avoiding pixel generation and GPU
reuploads. Resuming draws the current master timeline. The shared named
`animation_paused` entity is toggled by the zero-argument
`TodoActions::toggle_animation()` action.

`gui_ui.rs` supplies typed label, button, and checkbox props resolvers and
independent activation bindings. State initializes through `Default`. Separately
registered painters draw in local coordinates within fixed 36-pixel rows. The
application worker prepares components, paints, translates, clips, and emits
owned `RenderOutput`s containing `DisplayList`s. The GUI thread owns winit
windows and renderer-owned surfaces and executes the commands. Events identify
the displayed revision and return through the application queue. Stale clicks
are rejected rather than targeting an item at a changed position.

Todo row bindings currently resolve an
arena key by borrowed-value identity, with a linear search per row. Carrying
stable keys through loop contexts would remove that lookup and help implement
keyed component-state reconciliation.

Try an alternative button appearance without changing component behavior:

```sh
./t cargo run -p pixui-example-todo --bin gui -- --custom-painter
```

`custom_button_painter.rs` composes the standard painter with an accent strip.
The application chooses this painter instead of the standard button painter;
label and checkbox painters remain separately registered.

### Animated image component

The same GUI includes `OrbitingComets`, defined in `orbiting_comets.rs`. Its
custom painter manually generates a fresh 96 by 32 RGB snapshot on each worker
render: cyan and orange comets orbit with shrinking, dithered tails. Reserved
magenta pixels are transparent, revealing the row background in both themes.

State initializes through `Default`. Painting derives phase from the shared
`PaintContext::timestamp_us` timeline and calls `request_animation_frame()` for
another frame at the next native drawing opportunity. The host caps requests to
the monitor refresh rate, with a 60 Hz fallback when unavailable.
An explicit `PresentationSettings::timestamp_us` freezes or seeks the animation
and stops its redraw requests; `None` resumes the application clock. The GUI
schedules at most one outstanding request per window; animation keeps focus and
hover and permits clicks on compatible presented frames. Pixels are immutable
and shared through each display list's indexed image table. New frames release
old versions when outputs are dropped. The ordinary GUI command and
`--custom-painter` both include this component; no extra binary or image asset
is needed.

### Text atlas reuse

Labels use embedded Geist Regular TTF with coverage antialiasing. Tool-tool
fetches the pinned font when building through `./t`; the binary does not need
font files at runtime. Text uses real proportional advances and baseline
centering, with no wrapping. English and German windows share the worker's font
snapshot when face, size, and DPI match. Animating comets does not rasterize
unchanged glyphs again. A new todo with additional Latin characters extends the
snapshot while older outputs remain usable. Different DPI scales have separate
font resources. See [text drawing](../../crates/engine/src/ui/Text.md) for
limits and font licensing.

### Renderer selection

Auto uses femtovg with wgpu and reports a software fallback if initialization
fails. Both renderers use the same worker image resources and glyph atlases.
Choose explicitly for comparisons:

```sh
./t cargo run --release -p pixui-example-todo -- --renderer femtovg
./t cargo run --release -p pixui-example-todo -- --renderer software
```

`--renderer auto` is the default and combines with `--custom-painter`.
Renderer resources stay on the GUI thread, independently of component painters.
GPU texture caches reuse immutable snapshots and release expired/evicted
versions. See [renderer plugins](../../crates/gui/src/renderer/README.md).

Use `--freeze-animation` to fix the shared paint timestamp at zero and stop
animation redraws, useful for idle CPU comparisons and inspecting still frames.

Press **F11** in either window to toggle its performance overlay: FPS, worker
and renderer CPU stage timings, and referenced frame memory estimates. Idle FPS
is zero; refreshing diagnostics does not trigger worker painting.
