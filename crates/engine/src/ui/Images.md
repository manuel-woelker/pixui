# Dynamic images

`Image` is a `Resource<ImageData>` owning an immutable, row-major RGB snapshot
through the shared Arc-backed handle. Cloning shares
the allocation; construct a new snapshot to change pixels or metadata. Published
frames keep their exact versions until dropped, even if a newer frame replaces
them. Pixels, width, height, and the optional transparent color are readable.

```rust
use pixui_engine::ui::{display_list::Color, image::Image};
let image = Image::new(2, 1, vec![Color(255, 0, 0), Color(255, 0, 255)],
                       Some(Color(255, 0, 255))).unwrap();
let shared = image.clone();
assert_eq!(image.pixels().as_ptr(), shared.pixels().as_ptr());
```

Dimensions must be nonzero; the pixel count must exactly match width times
height and remain within `MAX_IMAGE_PIXELS` (64 million). This ceiling does not
bound all images or versions retained by the application. Construction consumes
the vector without copying. Equality compares snapshot identity, not pixel
contents; separately constructed equal pixels are distinct resources. Debug
output shows metadata without dumping pixels.

Color-key transparency skips pixels exactly equal to the reserved RGB color,
leaving earlier drawing visible. There is no partial alpha or blending. `None`
makes every pixel opaque, including magenta. Visible artwork must avoid its key.

## Shared command construction

There is one `DisplayListBuilder` per render. Its typed image table owns one
reference per distinct snapshot; a reverse map deduplicates by allocation
identity. It retains the allocation before caching that identity, preventing
address reuse. Finishing discards the map and returns a complete `DisplayList`
and optional redraw delay. Each `DrawImage` contains an `ImageIndex` scoped to
that list. Invalid indices are rejected before rasterization.

Painters call `context.image(&image, destination)` using local logical
coordinates. The context registers the image and translates the rectangle
directly into the shared builder. Components have no local display lists.
Renderer clips protect the component and viewport; painter clips must balance
independently. Errors discard all partial commands and resource references and
preserve published output. Previously executed state updates are not rolled
back.

The CPU painter stretches the full source into the destination with nearest-
neighbor sampling at physical pixel centers. Nested clipping limits writes
without changing source mapping. Zero-sized destinations do nothing. Choose
destination dimensions to preserve aspect ratio; the API does not infer layout
or crop images.

Sharing avoids repeated source copies across frames and windows, while CPU
sampling, framebuffer allocation, and native presentation still run on redraws.
A painter that changes pixels every frame must allocate a new immutable version.
For unchanged images, retain snapshots in application data and clone them into
props. No image registry, upload queue, or GUI cache is required initially.

## Animation scheduling

`context.request_animation_frame()` requests continuous animation. Requests
combine across painters into `RenderOutput::animating`. Call it on every frame
that needs a successor; an output without the flag ends continuous animation.
Headless clients explicitly drive subsequent frames: publication starts no
timer.

The native host presents an output, then requests the next native drawing
opportunity. At that opportunity it sends `UiCommand::AnimationFrame` with an
increasing request ID. `RenderOutput::animation_request` acknowledges the latest
request actually incorporated into a completed worker render. Only that output
releases the outstanding request, so unrelated actions cannot accidentally queue
extra animation frames. One request may be queued, running, or awaiting output
per window. A slow worker skips achievable frames without accumulating ticks.

Both built-in renderers notify the window immediately before presentation.
Native callbacks and FIFO pace presentation where supported. A monitor refresh
interval caps unthrottled platforms (60 Hz if the monitor rate is unavailable),
measured from the previous request rather than adding worker time to each tick.
This is a portable cadence cap, not an exact vsync or presentation-time API.
Publication wakes the GUI event loop through a coalesced notification, without
polling output mailboxes every 16 ms. Suspension, occlusion and zero-size
windows pause scheduling; restoration presents retained or updated output and
rearms it.

`context.request_redraw_after(delay)` remains available for occasional updates.
The shortest delay wins, clamped between one millisecond and one day. Continuous
animation supersedes a delayed request in the same output. A delayed deadline is
consumed before queueing `UiCommand::Redraw` and rearmed only by newer output.
Failed renders publish no output and stop the handshake until another
invalidation succeeds. Pending worker command admission and replies still use a
finite retry timer, independently of output delivery.

Visual redraws preserve focus, hover, scroll, and existing action bindings when
geometry matches. Older presented revisions in the same visual sequence can
still activate those bindings, preventing animation from racing clicks. Content,
presentation, interaction, or geometry changes require a current revision again.
Visual redraws are for appearance, not changing application content or targets.

## Master timestamp

`PaintContext::timestamp_us` is a `u64` count of microseconds on the
application's rendering timeline. The default monotonic clock starts with the
application; the renderer samples it once before preparation and shares the
value with every painter in that render. Different instances use the same clock
epoch, but each render samples independently. The clock saturates at `u64::MAX`
instead of wrapping.

Set `PresentationSettings::timestamp_us` to `Some(value)` to freeze or seek time
for that instance. `None` resumes the application clock. Overrides may move
backwards or start at zero, so painters must not assume time always advances.
This controls drawing time, not queued actions or application state. Change
settings with the existing `UiCommand::Present` API.

```rust
use pixui_engine::ui::presentation::PresentationSettings;
let frozen = PresentationSettings {
    timestamp_us: Some(1_000_000), // Exactly one second.
    ..Default::default()
};
```

Use the supplied timestamp rather than calling `Instant::now()` or `elapsed()`
inside painters. This allows synchronized components and deterministic
snapshots. When converting to phase, reduce in sufficient precision before
converting to `f32`, avoiding large elapsed values losing small animation steps.

The todo example's `OrbitingComets` painter derives phase from the master
timestamp and emits a fresh 96 by 32 image. Its state remains a named `Default`
unit type; there is no independent clock per node. Automatic time requests
another frame at the next drawing opportunity; an explicit timestamp stops those
requests until settings change. Shrinking, dithered tails expose either window's
background without alpha. Retained output redraws reuse the snapshot; queue
delays skip ahead in time rather than slowing the orbit.

See [shared render resources](Resources.md) for typed indices, identity, and
table ownership.
