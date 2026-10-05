# Dynamic image drawing plan

Status: proposed. Shared immutable snapshots, an indexed image table, and direct
painting into one shared builder are agreed directions. This plan adds an
animated example and the minimal scheduling needed to exercise it.

## Goal

Draw runtime-created RGB images with width/height metadata and an optional
transparent color. Share unchanged pixels across commands, frames, and windows
without copying them. Components append directly into one display-list builder.
Demonstrate the API with a custom component whose painter generates a new image
on each animated render.

## Context and chosen ownership

The application worker prepares components and generates display lists. The GUI
thread rasterizes them on the CPU and presents through softbuffer. Each instance
has one pending output slot; newer outputs can replace undelivered frames.
Windows retain their last complete output for redraws.

Use immutable `Image` snapshots wrapping `Arc<ImageData>`. The display list owns
an image table; draw commands contain indices into it. Every output is complete
and does not depend on resource updates from previously delivered frames.
Dropping pending or displayed output releases its snapshots automatically.

An in-process channel moves values without serialization. Sharing avoids copying
pixel vectors and repeating ownership references for each command; it does not
avoid reading source pixels or drawing the framebuffer on every redraw.

## Considered alternatives

| Option | Benefits | Costs and reason for deferring |
| --- | --- | --- |
| Owned pixel vectors in commands | Straightforward ownership. | Reusing pixels in multiple frames/windows requires copies. |
| `Image` directly in each command | Simple; no image-index validation. | Repeats reference counting for each use; the agreed table resolves resources once per output. |
| GUI image cache and explicit acquire/release | Small commands; suitable for prepared pixels or GPU textures. | Needs versioning, reliable updates, recovery after dropped frames, and cleanup. Shared snapshots meet current needs. |
| Shared mutable buffers | Can edit pixels without replacement allocation. | Locks and inconsistent retained frames complicate rendering. |
| Buffer pools or region patches | Can reduce allocation for frequent large edits. | Must account for every retained frame before reuse; defer until measured workloads justify it. |
| Component-local display lists | Each painter has an isolated command buffer. | Requires image-table merges, command-index remapping, and extra buffers; use a shared builder instead. |

## Immutable RGB image API

Add a named engine UI module with `Image` and private `ImageData`. Metadata and
pixels are readable through accessors; published snapshots expose no mutation.

```rust,ignore
let pixels: Vec<Color> = draw_pixels(width, height);
let image = Image::new(width, height, pixels, Some(Color(255, 0, 255)))?;
let shared = image.clone(); // Shares the allocation, not a pixel copy.
```

`ImageData` contains `width: u32`, `height: u32`, row-major contiguous
`Vec<Color>`, and `transparent_color: Option<Color>`. Reuse the existing RGB
`Color` type. Do not promise its Rust layout as an external byte format.

Construction consumes the vector without copying. Validate nonzero dimensions,
checked multiplication/conversion, exact pixel count, and a finite maximum pixel
count. Proposed ceiling: 64 million pixels per image, matching the rasterizer's
current framebuffer ceiling. This does not bound total memory; revisit the value
before large-image workloads. Custom `Debug` prints metadata without pixels.

A transparent color is an exact RGB key. Matching pixels leave the destination
unchanged. `None` means fully opaque. Partial alpha and blending are deferred;
artists cannot display the key color opaquely within the same image.

Runtime changes create replacement snapshots. Ordinary action handlers can
replace images in application data; props resolution clones the current
snapshot. A painter may also construct transient images directly, as the demo
will do. Unchanged snapshots are reused. Old outputs retain their old version
even after application data is replaced or removed.

An application image registry is optional future convenience, not required in
this increment. Direct snapshots already support creation, replacement, and
automatic cleanup without another global lifetime or invalidation protocol.

## Shared display-list builder and reverse lookup

Keep resources with `DisplayList`, rather than only `RenderOutput`, so
standalone headless rendering and tests remain self-contained.

```rust,ignore
pub struct DisplayList {
    pub images: Vec<Image>,
    pub commands: Vec<DrawCommand>,
}

pub struct ImageIndex(usize);

pub enum DrawCommand {
    DrawImage { image: ImageIndex, destination: Rect },
    // Existing drawing commands.
}

struct DisplayListBuilder {
    images: Vec<Image>,
    image_indices: HashMap<ImageIdentity, ImageIndex>,
    commands: Vec<DrawCommand>,
    // Earliest next-frame request, if any.
}
```

`ImageIdentity` identifies the `Arc<ImageData>` allocation. Hash and compare
identity, not pixel contents. Use a private pointer identity derived from
`Arc::as_ptr`; no dereference or unsafe code is needed. Avoid exposing pointer
addresses as stable IDs. The table owns a strong reference before storing its
identity, so allocation reuse cannot collide during the builder's lifetime.

`image_index(&Image)` returns an existing entry or inserts one clone. Repeated
clones deduplicate; independently constructed equal images remain distinct.
Finish consumes the builder, producing the display list and redraw scheduling
metadata, and drops the reverse map. Validate every image index before drawing.
Display-list equality compares image snapshot identity, without scanning pixels.

The public painter helper hides indexing:

```rust,ignore
context.image(&image, destination);
```

Keep one builder per render. Painters do not create component-local display
lists. Their typed context borrows the shared builder, registers images, and
appends commands directly. A fresh render starts a fresh table, so transient
animation images are not retained in a global cache.

## Component preparation, placement, and clipping

Direct insertion needs the final row origin before painting; scroll clamping
currently depends on the total component count. Refactor into two stages:

1. Walk once, reconcile default state, resolve props once, execute updates once,
   and collect prepared nodes with owned props, component identity, and resolved
   activation bindings. Retain positional paths into the physical state tree,
   not component-local command buffers or state clones.
2. Calculate fixed content height and clamp scrolling. Reborrow each node's
   state by its path, dispatch its checked painter, and append directly into the
   shared builder at the known origin. All preparation finishes before any
   painting.

Document the phase-order change. The template copy remains private to the
render; state paths must resolve the same prepared component identity before
painting. No application or physical-state borrows escape rendering. Keep paths
internal; this is not a public retained render-tree API. Investigate a simpler
equivalent safe reborrow during implementation, but do not add an extra
props/update walk or clone state to obtain positions.

Rows remain 36 logical pixels high with 8 pixels spacing and 16 pixels padding.
Width is clamped to zero; image drawing adds no sizing or layout API.

`PaintContext` retains local coordinates. It translates rectangle, text, image,
and nested-clip commands as they enter the shared builder. Viewport and
component clips are controlled by the renderer. Painters may manage only their
own clips: reject a pop at local depth zero and unbalanced clips at painter
completion. The renderer's outer clip cannot be popped by painter commands.
Ensure direct `emit` and helper calls use the same guard. `with_clip` restores
its clip after an ordinary error. Discard the entire builder on rendering
failure and preserve the last published output, revision, and geometry; updates
are not rolled back.

## CPU image drawing

Draw the complete source into a logical destination rectangle with initial
nearest-neighbor scaling. Callers choose aspect ratio through the destination.
Cropping and interpolation modes are deferred.

- Validate finite, nonnegative destinations; zero width/height is a no-op.
- Apply window scaling and intersect nested, component, and viewport clips.
- Map visible physical destination pixel centers back through the original
  destination rectangle to source indices, clamping edge samples. Clipping must
  not rescale the remaining image.
- Skip source pixels equal to the transparent key; write other RGB colors in the
  existing softbuffer pixel format.
- Iterate only visible destination pixels. Never clone or repack the whole
  source per draw command. A future backend can prepare each table entry once.

A 1920 by 1080 RGB image contains about 6.2 MB of channel data. Sharing avoids
copying that source on every unchanged frame. If the source changes at 60 FPS,
about 373 MB/s of new channel data is still generated, before rasterization.
The demo deliberately uses a small source image.

## Animated custom component: orbiting comets

Add the component directly to the existing two-window todo GUI, alongside the
heading, add button, and todo collection.
Register `OrbitingComets` and its custom painter with the normal typed APIs;
reuse the existing GUI host. No central renderer branches or external assets.

Component design:

- Props contain theme-aware cyan/orange colors and an optional speed multiplier.
- Default state stores an animation start `Instant`. Each physical component has
  its own start; ordinary renders retain it.
- The read-only painter derives phase from elapsed time, generates a fresh
  96 by 32 RGB image, emits it, and requests another frame after about 33 ms.
- Use an explicit-time pure pixel-generation function for deterministic tests;
  the painter supplies elapsed time. Do not advance a counter on every paint.

Animation recipe:

1. Fill the image with reserved magenta `(255, 0, 255)` and set that transparent
   key. Keep all visible colors different from it.
2. Move a bright cyan comet along an ellipse centered at `(48, 16)` with radii
   approximately `(30, 10)`. Move an orange comet half a turn behind it.
3. Draw each bright head as a small filled disc. Draw about ten trailing discs
   at earlier angles, reducing their radius and using a stable checker/dither
   mask toward the tail. Tiny nearby sparkles can alternate with phase.
4. Add a sparse dotted orbit behind them. Transparency between dots and
   particles exposes the live component background.

The trails fade by reduced coverage, not fake alpha or background-colored
pixels. Their transparent holes work on both light and dark backgrounds. Use
integer pixel loops and simple sine/cosine position calculations; no particle
simulation or random state is required.

Place the 96 by 32 source centered in the fixed-height row, shrinking uniformly
when the available width is smaller. Show another row on a contrasting
background or a second themed window so transparency is visible. Optionally draw
the same snapshot twice in a row to demonstrate table deduplication; do not
regenerate it for the second draw. Each animated render produces one new
snapshot per component. Redrawing a retained output reuses that snapshot without
repainting the component.

## Minimal animation scheduling

The current worker renders on invalidation, not continuously. Add a focused API
for painter-requested redraws instead of running an unconditional render loop:

- `PaintContext::request_redraw_after(Duration)` records the earliest request
  across the render. Clamp to a sensible positive minimum to prevent a busy
  loop.
- Publish an optional redraw delay with `RenderOutput`. Timing is presentation
  metadata, not part of image identity or display-list equality.
- The GUI host converts a newly received output's delay to a deadline and
  includes the earliest window deadline in its existing event-loop wakeup
  scheduling.
- At a deadline, enqueue a dedicated redraw command using the existing
  nonblocking input admission/retry mechanism. Keep at most one outstanding
  animation request per instance until a newer output arrives; do not enqueue
  accumulated ticks.
- Worker redraw commands mark only that instance dirty and preserve focus,
  hover, scrolling, and action bindings. Do not use content invalidation, which
  clears positional interaction state. An output's newly published revision
  still participates in the existing stale-input checks.
- An output without a request stops scheduling. Closing a window cancels its
  deadline. Failed rendering must not create an unbounded retry loop.
- Headless callers can explicitly trigger redraws with controlled timing; no
  hidden timer thread is needed. The example is not a hard real-time system.

Animation uses elapsed time, so queue delays and dropped frames skip ahead
rather than making the comet run slowly. Multiple windows remain independent. A
general animation timeline, hidden-window policy, and frame-rate configuration
are future work. Verify that frequent revisions do not starve native activation
requests.

## Implementation checklist

- [ ] Finalize image limits, identity equality, builder API, and clip error
      reporting; document the preparation-before-painting lifecycle.
- [ ] Implement immutable images, read accessors, cheap clones, and identity
      keys.
- [ ] Add the image table, typed indices, reverse lookup, and consuming builder.
- [ ] Refactor component preparation and painting to use one shared builder with
      correct origins and renderer-owned clipping; remove local command buffers.
- [ ] Implement `DrawImage`, context helpers, index validation, and
      scaled/clipped nearest-neighbor CPU rasterization.
- [ ] Add optional redraw requests and bounded per-instance host scheduling that
      preserves interaction and stops on closure or absent requests.
- [ ] Implement and document the custom orbiting-comets component and painter.
- [ ] Update architecture and component API documentation, plain architecture
      XML, and a decision record; use the running watcher for the ignored SVG
      preview.
- [ ] Run `./n check` after each implementation unit.

## Verification checklist

- [ ] Test invalid dimensions, overflow, pixel count, source limits, and concise
      debug output.
- [ ] Verify shared allocation, image identity deduplication, distinct
      equal-pixel snapshots, invalid indices, and reverse-map lifetime through
      builder finish.
- [ ] Verify old output draws old pixels after replacement; dropping frames and
      closing windows releases their references. Test resource-independent new
      consumers and skipped intermediate outputs.
- [ ] Test exact RGB output, color-key transparency over contrasting
      backgrounds, command ordering, scales, fractional destinations, clipping,
      translation, up/downsampling, and zero-sized destinations.
- [ ] Verify preparation and update callbacks run once, state remains
      independent, and shared insertion preserves row positions, scroll
      clamping, and errors.
- [ ] Test painter clip underflow/unbalanced clips without permitting it to pop
      a renderer clip. Test failure discards all partial commands and image
      entries.
- [ ] Test redraw-request minimum and earliest-deadline selection, coalescing,
      queue-full retry, stopping requests, window closure, and retained
      focus/hover.
- [ ] Test animation pixel generation at explicit times: periodic motion,
      changing snapshots, transparent coverage, reserved-color avoidance, and
      source size.
- [ ] Verify animation revisions retain usable actions and last-good-output
      error behavior; reuse existing GUI scale and clipping tests.
- [ ] Manually inspect animated light/dark windows, resizing, interaction, and
      closure without animation requests leaking after shutdown.

## Assumptions and limits

- Full-image replacement and nearest-neighbor sampling are initial defaults.
- The optional registry/cache, partial updates, alpha channel, file decoding,
  GPU uploads, and general layout are deferred.
- Animation intentionally allocates a small new source image per frame. Pooling
  is unnecessary until a measured workload justifies its ownership complexity.
- Snapshot retention and an image pixel ceiling do not bound total application
  memory. Callers may retain old versions indefinitely.
- Preparing all components before painting is a deliberate lifecycle change;
  existing state and error tests must exercise it before implementation is done.
