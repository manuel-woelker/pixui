# Layered filesystems and image loading plan

Status: implemented. Resource lookup, image decoding, RGB/RGBA rendering and
the todo logo component are complete.

## Goals

- Load an image using a relative resource filename, such as `icons/add.png`.
- Compose sources so an application can override resources from another source.
- Keep resource lookup independent of image decoding and rendering.
- Allow future embedded and network sources without changing painter APIs.
- Reuse the existing immutable, shared `Image` snapshots and display list
  tables.

## Implemented design

### Resource filesystem

Add an engine `resources` module containing a small, object-safe filesystem
interface, a validated path type, and directory and layered implementations.
Keep it independent of UI types; image decoding lives in a separate module.
There is no need for another crate yet.

Public API (abbreviated):

```rust,ignore
pub trait ResourceFilesystem: Send + Sync {
    fn open(&self, path: &ResourcePath)
        -> PixuiResult<Option<Box<dyn std::io::Read + Send>>>;
}

pub struct DirectoryFilesystem { /* explicit root directory */ }
pub struct LayeredFilesystem { /* ordered sources */ }

impl LayeredFilesystem {
    pub fn new(sources: Vec<Arc<dyn ResourceFilesystem>>) -> Self;
}
```

`None` means the resource does not exist. An empty file returns a reader that
immediately reaches EOF. Each open returns an independent, owned reader; callers
do not need to retain a borrow of the filesystem. Other failures use the
existing error infrastructure with source and filename context. The reader need
not support seeking. Consumers enforce byte limits when reading, before decoding
or allocating an arbitrarily large resource.

Sources are read-only. Do not expose native file handles or add directory
enumeration, writing, or metadata APIs until a consumer needs them.

### Relative paths and directory roots

`ResourcePath` is a validated relative filename backed by `PixuiString`. Its
constructor accepts `impl Into<PixuiString>`; validation happens once, and
callers can borrow its string representation without allocating. It uses
slash-separated, case-sensitive logical filenames. Reject empty paths, absolute
paths, parent traversal, backslashes, drive prefixes and NUL characters. Reject
empty and `.` components as well, giving each accepted filename one spelling. Do
not interpret filenames as URLs.

Directory roots are explicit and resolved when the source is created, rather
than depending on the current working directory during each read. Native
filesystems may differ in case sensitivity; document that resource authors must
use exact case for portability.

Canonicalize resolved disk paths and reject symlinks that escape the root.
Treat this as protection against accidental traversal, not a sandbox against
concurrent hostile filesystem changes. Only a missing file or missing parent
directory counts as absence; permission failures and other I/O errors propagate.

### Layering semantics

Sources are searched in constructor order: the first source successfully opening
a file wins. For example,
`[user_overrides, application_assets, built_in_assets]`. Layered sources can
themselves contain layered sources.

Only absence falls through to the next source. An opening error stops lookup.
Once a reader has been selected, read errors, size-limit failures and decode
errors propagate without trying lower layers. A corrupt override cannot silently
expose a different image from a lower layer. An empty source list behaves like
an empty filesystem.

### Image loading

Provide a cheaply cloneable `ImageLoader(Arc<ImageLoaderInner>)` handle. Keep
`ImageLoaderInner` private; it owns the filesystem and immutable loading limits.
Expose `load(&ResourcePath)` and a convenience `load_str(&str)` returning
`PixuiResult<Image>`. Callers clone the loader directly without wrapping it in
an additional `Arc`. Clones share configuration, not a mutable image cache.

Use the [`image` crate](https://github.com/image-rs/image) with default features
disabled and explicit PNG and JPEG support. Detect formats from file contents;
filenames remain useful for diagnostics. Other formats and animation are out of
scope. Reject animated input where the supported decoder exposes it rather than
promising animation playback.

Use [decoder limits](https://docs.rs/image/latest/image/struct.ImageReader.html)
as well as encoded byte limits. Read at most the configured limit plus one byte
from the selected reader to detect oversized input. Decode the bounded buffer
using a cursor, supplying seeking locally rather than requiring it of sources.
Validate dimensions before allocating the final
pixel buffer, respect `MAX_IMAGE_PIXELS`, and use checked size arithmetic.
Convert decoded pixels into the existing `Image` representation and preserve
useful error context. A missing resource becomes an explicit loading error.

### Image pixel formats and transparency

Add an enum describing the image pixel storage, with RGB and RGBA variants.
Keep dimensions and shared snapshot identity in `ImageData`. Store pixels in the
variant so the format cannot disagree with the buffer representation:

```rust,ignore
pub enum ImagePixels {
    Rgb {
        pixels: Vec<Color>,
        transparent_color: Option<Color>,
    },
    Rgba {
        pixels: Vec<RgbaColor>,
    },
}

pub struct RgbaColor(pub u8, pub u8, pub u8, pub u8);
```

Preserve the existing RGB constructor and add an RGBA constructor. Both validate
pixel counts and dimensions. Replace RGB-only accessors at consumers with
explicit matching on the storage enum. Keep drawing colors independent of image
pixel formats; this change does not require changing `Color` throughout the UI.

Decode alpha-bearing images into RGBA without dropping partial transparency;
decode opaque images into RGB. Define RGBA as straight (unpremultiplied) alpha,
with channels in RGBA order and alpha ranging from zero to 255. Preserve RGB
color-key behavior for existing generated images. Update the software renderer
for source-over alpha composition and the femtovg upload path for the
appropriate alpha convention, avoiding double premultiplication. Font atlas
handling must continue to work through the same image resource mechanism.

Include format-aware memory accounting in performance diagnostics: RGB consumes
three bytes per pixel and RGBA four, plus storage overhead. Check all allocation
calculations for both variants. Supporting RGBA is part of this implementation,
not a deferred prerequisite.

### Ownership, reuse, and execution

Loading is synchronous. Explicit callers retain the returned `Image`; the
standard core `ImageComponent` takes a relative `ResourcePath` prop and resolves
it into its state during worker-side preparation. Painters register the prepared
snapshot in the shared display list. Do not read files or decode images during
painting or on the native UI thread. Loading in a worker action blocks that
worker; document this limitation.

Direct `ImageLoader` calls have no automatic cache. The application provides a
weak path cache that shares live snapshots across image components/windows and
releases pixels when state and outputs release their owners. Replacing the
application loader clears lookup and invalidates UIs. Retaining and cloning
the returned `Image` preserves resource identity and existing renderer caching.
Calling `load` again deliberately produces a new snapshot. Replacing an image
does not invalidate older render outputs; their shared ownership keeps the old
snapshot alive until released. Automatic watching and hot reload are out of
scope.

### Embedded and network sources later

An embedded source can implement the same interface using a cursor over static
bytes, avoiding a source-side copy. The image loader still creates a bounded
decode buffer. Network implementations can expose streaming readers, but must
respect the execution constraints below.

The synchronous interface is deliberately not a promise of nonblocking network
I/O. A future network integration should fetch on a background executor and
publish completed resources to a local source or dispatch a loaded snapshot to
the application. Pending fetches must not masquerade as missing resources and
trigger incorrect fallback. If callers need asynchronous lookup directly, add
an asynchronous loading interface with explicit pending/error semantics then.
Keep HTTP caching, retries and authentication inside that integration.

## Alternatives considered

| Option | Reason for choosing another approach |
| --- | --- |
| Load directly from paths in painters | Repeated I/O and decoding would delay rendering and bypass snapshot reuse. |
| Filesystem returns decoded images | Couples source implementations to one resource format and duplicates decoding. |
| Fall back after any error | Hides broken overrides and makes resource selection unpredictable. |
| Build a full virtual filesystem | Enumeration, mutation and mount routing have no current consumer. |
| Make all loading asynchronous immediately | Adds scheduling and cancellation complexity before there is a network source. |
| Cache every decoded image automatically | Adds invalidation and retention policy; callers can retain snapshots already. |

## Implementation checklist

- [x] Add documented `ResourcePath` validation and the filesystem interface in
  named modules; keep `mod.rs` and `lib.rs` limited to module declarations.
- [x] Implement rooted directory opening with owned readers, containment checks,
  and useful errors.
- [x] Implement ordered, nestable layered lookup and document fallback behavior.
- [x] Add the RGB/RGBA storage enum, constructors, renderer support, font atlas
  compatibility, and format-aware memory accounting.
- [x] Add the decoder dependency, bounded reading, decoding limits, image
  conversion and the cloneable loader handle with convenience API.
- [x] Load the supplied logo at `assets/images/pixui-logo.png` during todo
      component preparation, using the relative resource filename
      `images/pixui-logo.png` with `assets` as the filesystem root. Add an image
      component to the todo UI whose painter draws the retained snapshot,
      preserving its aspect ratio and transparency. Demonstrate overriding the
      logo with an optional higher-priority directory.
- [x] Update `crates/engine/src/ui/Images.md` and `docs/Architecture.md` to
      explain source lookup, decoding, snapshot ownership, and which thread
      performs loading.
- [x] Run `./n check` after each completed unit and resolve introduced failures.

## Verification

- [x] Path tests cover valid nested names and each rejected path form,
  including Windows-style absolute and traversal paths on every platform.
- [x] Directory tests cover independent readers, exact bytes, empty files,
      absence, invalid roots and errors. Loader tests cover bounded reads and
      read errors. Exercise symlink containment where supported.
- [x] Fake source tests verify priority, absence-only fallback, error
      propagation, nested layers, and no reads of lower sources once a match is
      found.
- [x] Decode fixtures cover PNG and JPEG, malformed/truncated input, unsupported
      formats, dimension/allocation limits, missing resources, opaque RGB,
      binary transparency, and partial RGBA transparency. Use generated fixtures
      where possible and keep binary fixtures small.
- [x] Renderer tests verify RGB color keys and RGBA alpha values of zero,
      partial and full opacity over known backgrounds in both backends. Verify
      font atlases and memory accounting for both storage variants.
- [x] Integration tests load an override and verify that drawing retains the
  resulting snapshot, repeated registration shares its resource index, and an
  older output remains valid after replacing the loaded image.
- [x] Verify the todo logo visually (user confirmation), with automated
  aspect-ratio and override tests and required GPU/software blending tests.
  Run automated tests through the existing nextest/check tasks.

## Assumptions and implementation notes

1. Default encoded-size and decode buffer limits are 32 MiB and 256 MiB,
   configurable through `ImageLoadLimits`, with the engine pixel limit as an
   upper bound. Decoder internal allocation limits are best effort; encoded data
   and allocator overhead are additional memory.
2. Directory roots and source order are configured explicitly by the
   application. Application-wide resource registration and implicit painter
   access are not needed for this first version.
3. Component preparation may synchronously load a cache miss. Responsive loading
   of large runtime assets will need background work later.
4. Following review, `ImageComponent`, `ImageProps`, `ImageState` and
   `ImagePainter` are standard core APIs. Props carry a resource path, not an
   already-loaded snapshot. `Component::prepare` resolves resources before
   painting through an application-owned weak cache. Capturing props resolvers
   are unnecessary; the existing function-pointer API remains unchanged.
5. Todo assets are located relative to its source checkout at setup. Packaging
   resources with an installed binary remains a separate deployment concern.

## Verification results

- `./n check` passes, including nextest, documentation tests, formatting and
  clippy.
- Required GPU tests (`PIXUI_REQUIRE_GPU=1`) pass, including RGB color keys,
  partial RGBA alpha, glyph atlases, clipping, DPI and texture cache behavior.
- The user confirmed that the todo UI looks good. Override selection, corrupt
  override errors, logo aspect ratio and snapshot reuse are also checked
  automatically; native override appearance was not separately confirmed.
- Follow-up verification covers path changes, shared cache hits, weak ownership,
  missing loader errors, failed loads, and loader replacement invalidating UIs.
