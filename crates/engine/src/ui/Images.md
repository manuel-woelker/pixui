# Dynamic images

`Image` is a `Resource<ImageData>` owning an immutable, row-major RGB or RGBA
snapshot through the shared Arc-backed handle. Cloning shares the allocation;
construct a new snapshot to change pixels or metadata. Published frames keep
their exact versions until dropped, even if a newer frame replaces them. Width,
height, and `ImagePixels` storage are readable. Match on the storage enum to
handle both formats; `rgb_pixels()` is a convenience for known RGB data.

```rust
use pixui_engine::ui::{display_list::Color, image::Image};
let image = Image::new(2, 1, vec![Color(255, 0, 0), Color(255, 0, 255)],
                       Some(Color(255, 0, 255))).unwrap();
let shared = image.clone();
assert_eq!(image.rgb_pixels().unwrap().as_ptr(), shared.rgb_pixels().unwrap().as_ptr());
```

Dimensions must be nonzero; the pixel count must exactly match width times
height and remain within `MAX_IMAGE_PIXELS` (64 million). This ceiling does not
bound all images or versions retained by the application. Construction consumes
the vector without copying. Equality compares snapshot identity, not pixel
contents; separately constructed equal pixels are distinct resources. Debug
output shows metadata without dumping pixels.

Color-key transparency skips pixels exactly equal to the reserved RGB color,
leaving earlier drawing visible. `None` makes every RGB pixel opaque, including
magenta. Visible artwork must avoid its key.

`Image::new_rgba` accepts `Vec<RgbaColor>` in RGBA channel order with straight
(unpremultiplied) alpha. Zero alpha leaves the background unchanged, 255
replaces it, and intermediate alpha blends source-over. The software renderer
blends in encoded sRGB with integer rounding; femtovg handles premultiplication
once when sampling. RGB uses three bytes per pixel and RGBA four; frame
diagnostics account for each format. Glyph atlases remain separate
single-channel coverage resources.

```rust
use pixui_engine::ui::image::{Image, RgbaColor};
let translucent = Image::new_rgba(1, 1, vec![RgbaColor(0, 128, 255, 128)])?;
# Ok::<(), pixui_base::PixuiError>(())
```

## Filesystem loading

[`ImageLoader`](crate::resources::image_loader::ImageLoader) decodes bounded PNG
and JPEG resources selected by relative filename. See
[resource filesystems](../resources/README.md) for source setup, layering and
limits. For explicit loading, retain the snapshot for reuse. The standard core
`ImageComponent` instead takes `ImageProps { path: ResourcePath }` and resolves
its snapshot during worker-side preparation. Painters and the native UI thread
perform no resource I/O.

Configure an application loader before rendering:

```rust,no_run
use std::sync::Arc;
use pixui_engine::{
    application::app::Application,
    components::image::ImageProps,
    live_model::part::ComponentPart,
    resources::{directory::DirectoryFilesystem, image_loader::ImageLoader},
};
let app = Application::new();
app.set_image_loader(ImageLoader::new(Arc::new(DirectoryFilesystem::new("assets")?)))?;
let components = app.register_standard_components()?;
app.register_standard_painters()?;
let image = ComponentPart::typed(components.image, |_, _| {
    ImageProps::new("images/pixui-logo.png")
});
# Ok::<(), pixui_base::PixuiError>(())
```

`ImageState` retains the prepared snapshot. The standard `ImagePainter` centers
it within the component dimensions while preserving its aspect ratio. Custom
painters can access the same prepared image through `context.state.image()`.
The application caches weak handles by resource path, sharing live snapshots
across components and windows without retaining unused pixel buffers. Changing
the path loads on demand. Replacing the application loader clears lookup and
invalidates windows; older outputs keep their original snapshots. Loading
failures preserve the last published output, and later renders can retry.

The todo UI loads `assets/images/pixui-logo.png` through the `assets` root and
shows it in an image component. Run with `--assets /path/to/overrides` to try a
higher-priority `images/pixui-logo.png`. Missing files fall back; invalid
overrides report an error. The example locates its default assets from the
source checkout; packaged applications should supply an explicit installed
assets directory.

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
props. The femtovg renderer caches uploaded textures by snapshot identity;
unchanged images need no new upload. Loading the same filename again creates a
new identity.

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
timestamp and emits a fresh 96 by 32 image while playing. Component state holds
only a `paused` boolean. The painter caches one image per theme and reuses its
exact snapshot while paused, avoiding pixel generation and repeated GPU uploads.
There is no independent clock per node; resuming uses the current master time.
The shared **Pause animation** / **Resume animation** button controls both
windows. Paused redraws do not request animation frames. Automatic time requests
another frame at the next drawing opportunity while playing; an explicit
timestamp also stops those requests until settings change. Shrinking, dithered
tails expose either window's background without alpha. Queue delays skip ahead
in time rather than slowing the orbit.

See [shared render resources](Resources.md) for typed indices, identity, and
table ownership.
