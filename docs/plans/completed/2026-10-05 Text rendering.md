# Text rendering plan

Status: implemented. The reviewed choices and verification results below
describe the completed worker atlas renderer.

## Goal

Replace bitmap-cell fonts with scalable antialiased text while keeping draw
lists compact. Send strings and font-resource indices, prepare glyph atlases on
the worker, and share immutable resources through `Arc`. Reuse unchanged font
snapshots across frames, including while the todo comets animate.

## Agreed decisions

- Draw commands contain text, origin, color, and a font-resource index. Do not
  transmit positioned glyphs per character occurrence in the first
  implementation.
- Each font resource contains one coverage atlas and a Unicode `char` lookup to
  `GlyphInfo`, plus line metrics. A resource represents one face, logical size,
  and rasterization scale. No multiple atlas pages initially.
- Resources and atlas pixels are immutable snapshots shared through `Arc`.
  Retained frames keep the exact atlas and character map they were published
  with.
- Collect missing characters during painting and prepare them in a batch before
  finishing/publishing the display list. Finalized resource indices serve all
  commands regardless of painter order.
- Use `fontdue` for metrics/rasterization and `etagere` for rectangle packing.
  Keep library-specific types private to the implementation.
- Define our own metrics, pixel rectangles, atlas resources, and cache
  identities. Separate font loading, glyph rasterization, and atlas packing into
  named modules.
- Introduce an internal concrete atlas builder, without runtime backend
  selection, plugin registration, or a backend trait until a second
  implementation is needed.
- First scope is character-based horizontal text. Contextual shaping, ligatures,
  general bidi, rich text, editable controls, and color emoji are deferred.
  Character-based lookup is a deliberate limitation, not general Unicode layout.
- Keep existing fixed-height component rows, local-coordinate translation,
  clipping, latest-output delivery, and last-good-output failure behavior.

## Previous implementation

The engine emits `DrawText` with a string, position, bitmap font ID, size, and
RGB color. The GUI iterates characters and paints each set bit of an 8 by 8
glyph. Unsupported characters become `?`; metrics and the old wrapping helper
assume square character cells.

Painters already append directly into one shared display-list builder. Images
use immutable `Arc` snapshots and indexed tables. Their RGB color-key
transparency cannot express partial coverage, so text needs a grayscale coverage
atlas and compositor; the RGB image API remains unchanged.

## Resource and command contracts

Illustrative types; finalize names and exact public signatures during
implementation:

```rust,ignore
struct FontResource {
    atlas: Arc<GlyphAtlas>,
    characters: HashMap<char, GlyphInfo>,
    ascent: f32,
    descent: f32,
    line_height: f32,
    // Rasterization scale; cache configuration retains face and logical size.
}

struct GlyphAtlas {
    width: u32,
    height: u32,
    coverage: Vec<u8>,
}

struct GlyphInfo {
    atlas_rect: Option<PixelRect>,
    advance: f32,
    offset: Point,
}

DrawText {
    font: FontIndex,
    text: String,
    origin: Point,
    color: Color,
}
```

The completed display list owns `Vec<Arc<FontResource>>`. Font indices are local
to that list. No worker-private registration ID or earlier upload message is
required to draw it. Metadata and maps are read-only through public accessors.

Atlas rectangles are physical integer pixels. Advances, offsets, ascent,
descent, and line height use logical pixels. Fontdue's bitmap coordinates and
bearing conventions must be translated explicitly to our downward-positive
coordinates. Spaces have an advance and can have no atlas rectangle. Empty text
is valid. The GUI advances the pen using metrics, not atlas rectangle widths.

Recommend defining command origin as the first baseline. Painters then
vertically center text using real ascent/descent metrics. If preserving the
current top-left origin is preferred, define a consistent baseline conversion in
one place. Never guess baseline placement independently on the worker and GUI.

## Worker service and batch finalization

Use a worker-owned text service storing parsed faces and current atlas
snapshots. Painters access it through the render's context/builder, never
through blocking calls to their own application handle. Immutable resource types
contain no third-party parser/allocator objects.

1. A text helper resolves a font face, logical size, and scale to a render-local
   `FontIndex`. Repeated requests for that configuration reuse the same index.
2. Append the compact text command and collect required characters in a set for
   that font. Exclude newline and other explicitly handled control characters.
3. If a painter requests text metrics, obtain/cache glyph metrics immediately.
   Preparing coverage pixels can wait until after all painters finish.
4. During finalization, rasterize each missing drawable glyph once, allocate
   atlas rectangles as a batch, and create one replacement resource per changed
   font.
5. Install the finished snapshots in the display list, validate references and
   resource bounds, and publish only after every font has finalized
   successfully.

The batch is a collection/packing operation: fontdue rasterizes individual
glyphs, so do not imply it provides a multi-glyph rasterization API. Existing
characters reuse cached metrics and pixels. Missing source-font characters
should alias one prepared replacement glyph instead of rasterizing duplicates
for every codepoint.

At most one replacement snapshot per changed font configuration per render.
Unchanged resources reuse their existing `Arc`. A new snapshot includes all
characters accumulated for that font configuration; font cache limits and atlas
limits prevent unlimited growth.

## Single-atlas growth and snapshot lifetime

Adding pixels to a published atlas requires a new allocation even when empty
space remains. Spare capacity reduces repacking/growth frequency, not immutable
copying. Maintain a mutable packing state only on the worker; snapshot metadata
and pixels remain consistent for every retained output.

Suggested initial policy:

- Start with a modest bounded atlas; include padding around drawable glyphs.
- Append into free rectangles when possible, copying current atlas pixels once
  for a changed batch and retaining existing glyph coordinates.
- When capacity is exhausted, grow geometrically up to a configured maximum and
  repack the complete glyph set if necessary. Update rectangles in the new map.
- Use deterministic character order and packing inputs for reproducible tests.
- If the maximum cannot fit the batch, return a descriptive error, preserving
  the last good output. Multiple pages or eviction within a font are future
  choices.

Do not deep-clone an atlas per text command or per glyph. Validate dimension
multiplication, source rectangles, coverage lengths, font indices, finite
metrics, font sizes, and scale before expensive allocations or drawing.

New font characters can change the snapshot without changing the font's logical
identity. Font bytes, face index, size, scale, and rasterization settings must
participate in cache identity; replacing a face must not reuse old glyph data.
Frame snapshots retain old rectangles even after repacking. Dropped frames and
closed windows release references automatically; cache eviction releases only
cache ownership, not references held elsewhere.

## GUI drawing and antialiasing

The GUI reads the string and its finalized font resource:

- Look up each character's `GlyphInfo`, draw its atlas rectangle at pen plus
  offset, then add its advance.
- Newlines reset horizontal position and advance by resource line height.
- Apply translated command origin and existing nested/component/viewport
  clipping.
- Blend 8-bit coverage with the command's RGB foreground and destination pixels.
  Coverage zero preserves the destination; 255 writes the foreground.
- Use rounded channel interpolation initially in encoded RGB, documenting that
  this is not linear-light blending. Defer LCD subpixel antialiasing.
- Rasterize for the target physical font size. Do not upscale a low-resolution
  atlas with image nearest-neighbor sampling to simulate high-DPI text.

Different scales require distinct atlas resources. A retained output drawn at a
new window scale must remain safe; document whether it is temporarily resampled
until the matching worker output arrives or withheld while new output is
pending.

Text does not require image color keys or background-colored glyph edges.
Uniform color is supplied per command, letting one atlas serve both themes and
hover colors. The GUI performs no font parsing, glyph rasterization, or atlas
allocation.

## Text scope and measurement

The first renderer uses input character order and per-character advances. It
does not claim correct Arabic shaping, arbitrary combining sequences, ligatures,
or mixed-direction layout. Font coverage alone does not solve these limitations.

Recommend no-wrap plus existing clipping initially, with explicit newlines.
Remove or clearly isolate the old character-cell wrapping estimate. Wrapping or
ellipsis can be added later with a defined algorithm shared by measurement and
rendering; they must not silently change component row heights.

Kerning is an open decision: a `char -> GlyphInfo` map cannot hold
pair-dependent adjustments by itself. Omit it explicitly initially, or extend
the resource with pair adjustments used identically by the worker and GUI. Avoid
measuring kerned text while rendering only unadjusted advances.

The painter API can remain string-based even if shaped text is introduced later.
A future shaped-run command would use font-specific glyph IDs and positions,
requiring an additional representation rather than a rasterizer plugin alone.

## Private library boundary

An internal concrete `FontAtlasBuilder` exposes operations such as metrics
lookup and batch preparation. Our types define all inputs and outputs.
`fontdue::Font` and `etagere` allocators remain inside their respective named
modules.

Replacing the packer later affects allocation/growth logic. Replacing the
rasterizer affects parsing and translating glyph metrics/coverage. Extract a
trait from those actual differences when implementing a second backend; do not
add speculative factories or flags now.

Primary references checked during planning:

- [Fontdue rasterization API](https://docs.rs/fontdue/latest/fontdue/struct.Font.html#method.rasterize)
  returns metrics and 8-bit glyph coverage. Its metrics-only API supports
  obtaining placement before rasterization.
- [Fontdue repository](https://github.com/mooman219/fontdue) documents its
  rasterizer scope and lack of an integrated shaping engine.
- [Etagere documentation](https://docs.rs/etagere/latest/etagere/) describes
  shelf-based rectangle allocation. It packs rectangles; we manage pixel
  copying, immutable snapshots, growth, and finalization.

Pin released versions and validate these APIs during implementation. Avoid
treating upstream benchmark claims as evidence for this application's
performance.

## Choices reviewed before implementation

| Decision                          | Suggested initial default                                                            | Why it matters                                                                                   |
|-----------------------------------|--------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| Bundled font and styles           | Use Geist, Latin subset                                                              | Reproducibility, coverage, and required attribution.                                             |
| Runtime font loading and fallback | Bundled face with one missing-glyph placeholder; defer system discovery              | Smaller API and deterministic tests; limited coverage must be documented.                        |
| Text origin                       | Baseline origin                                                                      | Makes glyph bearings and vertical alignment explicit.                                            |
| Kerning                           | Omit initially and document it                                                       | Pair adjustments need additional resource data and matching measurement.                         |
| Wrapping/ellipsis                 | No-wrap with clipping; explicit newlines                                             | Keeps compact commands and fixed-height rows simple.                                             |
| Tabs/control characters           | Normalize CRLF; choose a fixed tab policy or reject unsupported controls             | Prevents measurement and drawing divergence.                                                     |
| Atlas dimensions and growth limit | Small initial atlas, geometric growth, finite pixel/dimension ceiling                | Immutable copies and repacking can become expensive. Pick concrete limits before coding.         |
| Padding and subpixel position     | One-pixel padding and a documented pixel-snapping rule                               | Prevents glyph bleed and inconsistent cache/drawing behavior.                                    |
| Cache limits                      | Bound font configurations and atlas memory; simple eviction                          | Animated/variable sizes must not grow caches indefinitely. Retained frames may outlive eviction. |
| DPI transition                    | Safely resample retained atlas until matching output arrives                         | Avoids blank text during resize while retaining old-output guarantees.                           |
| Finalization failures             | Discard frame; retain usable cache entries, but commit each font snapshot atomically | Avoids mismatched pixels/maps and unnecessary full cache rollback.                               |

## Reviewed implementation choices

Use static Geist Regular 1.7.0 TTF from the official release, downloaded and
cached by tool-tool. Embed its bytes at build time using a tool-tool supplied
path; built executables need no font download or cache directory. Retain the
SIL Open Font License and attribution in the repository. The official TTF is
not byte-subsetted: restrict the initial glyph repertoire to Latin (U+0020–024F)
and the replacement character. This avoids adding a font-subsetting toolchain.

Use baseline origins, no kerning, no wrapping, explicit newlines. Normalize
CRLF/CR to LF and tabs to four spaces; reject other control characters. Use one
pixel padding and round glyph origins to physical pixel boundaries. Retained
outputs may temporarily resample coverage at a new DPI until rerendering.

Start atlases at 128 square pixels, double up to 2048, and bound physical font
sizes to 256 pixels. Bound the worker cache to 16 configurations and 32 MiB of
coverage with least-recently-used eviction. Bound character aliases as well as
pixels. Each font snapshot commits atomically; a failed frame retains its prior
published output. Keep fontdue and etagere types private and use concrete
helpers.

## Implementation checklist

- [x] Resolve font asset/license, origin, control-character policy, kerning
      scope, atlas limits, cache limits, and DPI transition behavior.
- [x] Define our font handles, logical metrics, pixel rectangles, `GlyphInfo`,
      coverage atlas, immutable font resource, and local font indices.
- [x] Implement private font loading and fontdue metric/rasterization adapters;
      add the bundled asset with its license and attribution.
- [x] Implement etagere packing, deterministic batch preparation,
      growth/repacking, and transactional publication of each font snapshot.
- [x] Add the worker text service and builder's font reverse lookup/missing
      sets; finalize all font resources after painting before output
      publication.
- [x] Update compact text commands and painter measurement/drawing helpers.
      Migrate label/button/checkbox painters to real metrics and the agreed
      origin.
- [x] Implement GUI character lookup, advances/newlines, DPI handling, clipping,
      and grayscale coverage blending.
- [x] Exercise font growth and reuse in the existing animated two-window todo
      GUI; remove bitmap-cell assumptions from production rendering.
- [x] Update API/architecture documentation and plain architecture XML, leaving
      SVG export to the running watcher. Record accepted decisions and explicit
      limits.
- [x] Run `./n check` after each implementation unit.

## Verification checklist

- [x] Test invalid font bytes, dimensions, coverage length, indices, source
      bounds, sizes/scales, empty strings, and configured limits.
- [x] Verify missing characters from multiple painters deduplicate and all
      commands resolve the same finalized resource regardless of painter order.
- [x] Verify unchanged frames reuse snapshots without pixel
      copying/rasterization; changes in color/origin do not generate a new
      atlas.
- [x] Test one replacement per changed configuration, growth/repacking,
      overflow, spaces, missing-glyph aliasing, newline/control policy, and
      consistent metrics.
- [x] Verify old frames survive new characters, repacking, cache eviction,
      dropped intermediate frames, and new consumers without upload history.
- [x] Test coverage zero/255/intermediate values on contrasting backgrounds,
      clipping, translations, bearings, negative offsets, physical scale, and
      snapping.
- [x] Test atlas/configuration reuse across windows and misses for
      font/face/size/ scale changes. Verify retained-output behavior during a
      DPI transition.
- [x] Verify finalization errors preserve last-good frame/revision/geometry and
      cannot publish a partial atlas/map combination.
- [x] Use pinned fonts for reproducible metric/pixel tests; document unsupported
      shaping and bidi behavior rather than testing them as implemented
      features.
- [x] Inspect English/German rendered frames at 1× and 2× in both themes. Launch
      native windows and exercise actions/animated resource reuse in tests.
      Confirm unchanged text does not rebuild atlases.

## Implementation and verification results

Implemented with fontdue 0.9.4 and etagere 0.2.15. Our public font resources
contain no parser/allocator types. Tool-tool downloads/checksums Geist 1.7.0;
its static Regular TTF is embedded at build time. The font license is retained
in `docs/licenses/Geist OFL.txt`. See `crates/engine/src/ui/Text.md` and DR-007
for exact contracts and limitations.

The worker cache uses configuration identity and bounded LRU ownership. The
builder collects demands for all painters and finalizes them together.
Alias-only changes share coverage; growth preserves old snapshots and existing
pixels. Glyph dimensions and total padded area are checked before allocating
an impossible batch. Finalization failures retain the last published revision,
geometry, and resource. A worker-local RefCell permits cache updates through
the renderer's immutable application view without sharing application state.

Tests cover invalid metadata/font data, batching/order/deduplication, metrics
and controls, one replacement glyph, append/growth, Arc reuse, LRU and byte
budgets, old snapshots, failed publication/recovery, exact coverage blending,
bearings, newlines, clipping, snapping, and DPI transitions. The animated todo
example verifies cross-window resource reuse and atlas growth after adding
Latin characters. Manually inspected rendered English/light 1× and German/dark
2× frames; native windows were also opened for optional user smoke testing.
Native interaction and animation behavior are exercised by automated tests.
The user confirmed successful manual UI verification on 2026-10-05.

Ran `./n check` throughout implementation. Updated architecture documentation,
API guides, and the existing draw.io XML; SVG export stays with the watcher.
