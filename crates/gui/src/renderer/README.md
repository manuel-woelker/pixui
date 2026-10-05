# GUI renderer plugins

The application worker creates immutable `DisplayList`s. Component painters
produce commands; GUI renderers execute and present them. GPU textures, devices,
and surfaces stay on the GUI thread. The engine does not depend on femtovg.

`host::run` selects Auto. `host::run_with_renderer` selects Auto, Software, or
Femtovg. Auto tries GPU initialization, logs a failed attempt, and falls back to
software for that window. Auto also chooses software when the available adapter
is a CPU implementation. Explicit Femtovg selection returns failure instead.
`host::run_with_factory` accepts a custom `RendererFactory`; its `create` method
receives an `Arc<Window>` and returns one `Box<dyn Renderer>` per window.
Renderers have no Send/Sync requirement.

## Contract

`resize` receives physical dimensions and a logical-to-physical scale.
Zero-sized windows do not render. `render` consumes borrowed commands and
returns Presented or Skipped. The host advances its input revision only after
Presented; Skipped retains the previous revision and retries after at least 33
ms. Errors do not advance the revision. Suspension releases native surfaces,
clears the presented revision, and cancels retries/animation deadlines. Resume
recreates the surface and requests an updated worker presentation.

Femtovg shares a GPU device and queue between windows created by its built-in
factory. Each window owns its canvas, surface, and texture cache. Lost/outdated
surfaces are reconfigured; timeouts/occlusion skip with bounded retries. Device
loss and GPU errors are returned on rendering. Fatal native errors currently
stop the host; runtime backend switching is not implemented.

## Text and images

Both implementations use worker `FontResource` snapshots. Shared glyph placement
handles baseline offsets, advances, spaces/newlines, physical pixel snapping,
and atlas resampling during DPI transitions. Femtovg's font layout is disabled:
coverage bytes become Gray8 textures and custom glyph quads sample the red
channel, tinted by the command color. Nearest sampling matches the CPU painter.
A tiny texel bias prevents boundary roundoff selecting an adjacent coverage
column. Images upload RGB converted to RGBA; the transparent color gets zero
alpha. Resources are not converted on every frame.

Solid rectangle bounds snap outward to match CPU touched-pixel coverage, and
strokes remain inside their rectangle. Nested clips intersect and restore.
Subpixel clip edges can differ from software due to GPU coverage; screenshots
need explicit tolerances. A non-sRGB surface format is preferred to match
encoded RGB blending. On surfaces that only support sRGB formats, blending can
differ.

## Cache lifetime

Caches use snapshot allocation identities with retained Weak references, never
frame-local indices. A Weak reserves its allocation address; dead entries are
pruned without keeping payloads alive. Retained snapshots remain immutable and
can be reuploaded after eviction. Old outputs continue to reference old atlases.

Each window has a 64 MiB texture budget with least-recently-used eviction. A
frame's working set can exceed the budget until submission; trimming follows.
Weak cleanup also removes dead resources. Image handles are released on eviction
and canvas drop; in-flight GPU commands retain the underlying textures.
`FemtovgRenderer::cache_stats` reports cumulative uploads, resident texture
count, and source-format texture bytes. Driver overhead and surface memory are
excluded.

## Validation and profiling

Run ordinary repository checks with `./n check`. GPU tests attempt an adapter
and print an unavailable reason when none exists. Require validation explicitly:

```sh
PIXUI_REQUIRE_GPU=1 ./t cargo-nextest nextest run -p pixui-gui renderer::scene::tests
```

The release profiling fixture is intentionally ignored in normal runs:

```sh
PIXUI_REQUIRE_GPU=1 ./t cargo-nextest nextest run --release -p pixui-gui \
  profile_release_renderer_phases --run-ignored only --success-output immediate
```

This measures command translation/upload and CPU submission separately from CPU
rasterization, using an offscreen target. It does not measure worker traversal,
compositor latency, or end-to-end GPU execution time. For actual GUI comparisons
use identical window sizes, DPI, animation rates, and release builds. GPU
drawing does not remove worker preparation. Output publication wakes the GUI
event loop immediately; only pending command admission/replies retain a finite
retry timer.

## Performance overlay

Press **F11** to toggle diagnostics in the focused window. The overlay is
anchored to the bottom-right corner with an eight logical pixel margin. It is
drawn last, has no hit regions, and works with both built-in renderers. Its font
atlas uses embedded Geist Mono, is prepared once per DPI scale and reused.
Numeric values use fixed-width columns with three decimal places for timings.
While visible, it refreshes at 4 Hz without scheduling worker renders; hiding it
stops those diagnostic redraws.

FPS counts distinct worker output revisions successfully presented during the
last second. Diagnostic refreshes, failed submissions, and skipped presentations
do not increase it. An idle application therefore shows zero FPS. Timings show
the latest presented application frame, so they can differ in age from the FPS
window. The overlay shows these CPU stages:

- **Prepare:** component validation, expression evaluation, state updates,
  props preparation and fixed-row geometry.
- **Paint:** painter calls and shared display list construction.
- **Text / finalize:** glyph batching, atlas creation, resource indexing and
  command validation.
- **Acquire / validate:** renderer validation and GPU surface acquisition.
- **Resources / upload:** GPU texture conversion and upload recording; zero
  for the software backend.
- **Draw / rasterize:** GPU command translation or software pixel rendering.
- **Submit / present:** GPU encoding, queue submission, presentation and cache
  cleanup; software surface resize, buffer acquisition, copying and
  presentation.

GPU numbers measure CPU work and may include surface waits. They do not measure
GPU execution or compositor latency. Custom renderers can expose timings through
`Renderer::timings`; otherwise the overlay reports them unavailable.

Memory figures estimate the current application's display list storage,
referenced RGB image allocations, and font metadata plus glyph coverage atlases.
Repeated snapshot handles and shared atlases are counted once per frame. Vector
capacities are included; font map storage is estimated from capacity. These are
not process RSS: allocator/Arc overhead, parsed font faces, worker caches, GPU
textures and the overlay itself are excluded. Resources shared between windows
are counted in each window's frame.

## Animation pacing

Painters request `request_animation_frame()` on every frame needing a successor.
After successful presentation the host schedules the next native redraw
opportunity, then asks the worker for one frame. The returned request ID is
acknowledged only in completed output; unrelated action output cannot create
multiple outstanding animation requests. The existing latest-output mailbox
still replaces stale outputs and worker batches remain bounded.

Both built-in backends call `Window::pre_present_notify()` immediately before
presenting, enabling Wayland frame-callback throttling. Femtovg retains FIFO.
For platforms with unthrottled native redraw events, the host caps requests at
the current monitor's refresh interval (60 Hz when unknown). Scheduling uses the
previous request time, includes worker work in the interval, and skips missed
intervals. This does not provide exact vsync or predicted presentation
timestamps. Each window schedules independently; suspended, occluded and
zero-size windows pause requests. Resume or restoration rearms scheduling from a
presented output.

`OutputReceiver::set_waker` attaches publication/disconnection notifications.
The host coalesces them with one shared atomic flag and an event-loop proxy,
clearing the flag before inspecting mailboxes to avoid lost wakeups. Idle
windows use `ControlFlow::Wait`; delayed updates, overlays, surface retries and
pending worker commands supply deadlines when needed. Headless consumers need no
waker and explicitly drive animation through commands.
