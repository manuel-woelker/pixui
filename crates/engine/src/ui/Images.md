# Dynamic images

`Image` owns an immutable, row-major RGB snapshot through `Arc`. Cloning shares
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

There is one `DisplayListBuilder` per render. Its image table owns one reference
per distinct snapshot; a reverse map deduplicates by allocation identity. It
retains the allocation before caching that identity, preventing address reuse.
Finishing discards the map and returns a complete `DisplayList` and optional
redraw delay. Each `DrawImage` contains an `ImageIndex` scoped to that list.
Invalid indices are rejected before rasterization.

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

`context.request_redraw_after(delay)` asks for another worker render. The
shortest request across components wins; requests are clamped between one
millisecond and one day. `RenderOutput::redraw_after` is optional presentation
metadata, not part of display-list identity. Headless consumers decide when to
send `UiCommand::Redraw`.

The native host arms one deadline per window when a newer output arrives. A due
deadline is consumed before its redraw command enters the finite nonblocking
retry queue. No more ticks accumulate while waiting for output. Outputs without
requests stop animation; closing or suspending the window cancels its deadline.
A failed render publishes no new output, so that animation stops until another
invalidation retries. Resuming refreshes presentation and obtains a new output.

Visual redraws preserve focus, hover, scroll, and existing action bindings when
geometry matches. Older presented revisions in the same visual sequence can
still activate those bindings, preventing animation from racing clicks. Content,
presentation, interaction, or geometry changes require a current revision again.
Visual redraws are for appearance, not changing application content or targets.

The todo example's `OrbitingComets` state stores its start time through
`Default`. Its read-only painter calculates elapsed phase, draws a fresh 96 by
32 RGB image, and requests another frame after 33 ms. Shrinking, dithered tails
expose either window's background without alpha. Retained output redraws reuse
the snapshot; queue delays skip ahead in time rather than slowing the orbit.
