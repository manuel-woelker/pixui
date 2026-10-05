# Text drawing

Text commands contain a string, first baseline, color, and `FontIndex` into
`DisplayList::fonts`. Each table entry is an immutable `Font`
(`Resource<FontResource>`): character metrics, one grayscale coverage atlas, and
rasterization scale. A retained frame is sufficient for a new renderer; there is
no upload history.

## Painter API

Use `PaintContext::text` with a **baseline**, not the top-left corner of a cell.
Helpers return `PixuiResult` so invalid sizes, controls, or font data propagate.

```rust,ignore
let metrics = context.font_metrics(16.0)?;
context.text(
    Point { x: 8.0, y: metrics.centered_baseline(context.height) },
    "Hinzufügen",
    16.0,
    foreground,
)?;
```

`font_metrics` exposes logical ascent, descent, and line height. `measure_text`
returns the widest line and total line height in `Size::width/height`.
Measurement uses fontdue's metrics-only API, without allocating coverage, with
the same normalization and unkerned advances as drawing. No automatic wrapping
occurs; explicit newlines advance by line height. Current component rows remain
36 logical pixels high, and overflowing text is clipped.

The lower-level `DisplayListBuilder::text` accepts a validated `FontConfig`.
`FontFace::from_bytes` loads static TTF/OTF faces with independent identities;
clones retain identity. System font discovery, variable axes, and fallback
chains are not implemented. Library parser and allocator types stay private.

## Font acquisition and scope

Tool-tool downloads the pinned official Geist 1.7.0 release using the
platform-independent `default` download key. Cargo's build script copies
`fonts/Geist/ttf/Geist-Regular.ttf` from the host's tool-tool cache into
`OUT_DIR` for embedding. It uses tool-tool's `PIXUI_GEIST_DIRECTORY` when
available and otherwise resolves `.cache/tool-tool/geist-1.7.0-{host_os}`. IDE
builds therefore need no custom environment variable after the font is
downloaded. The cache version in `build.rs` must follow the tool-tool pin when
upgrading Geist.

Download first through `./t` if the font cache is missing; build failures report
that prerequisite explicitly. Build scripts perform no network access. Use
`./t` for development tools to retain the pinned Rust toolchain. Executables
need neither the cache directory nor a runtime download. The source TTF includes
more scripts; this initial renderer restricts coverage to U+0020–024F (Latin)
and U+FFFD. It does not run a separate byte-subsetting pipeline.

The font is distributed under the SIL Open Font License. Include
[the license and copyright notice](../../../../docs/licenses/Geist%20OFL.txt)
when distributing executables containing it. See the
[official release](https://github.com/vercel/geist-font/releases/tag/1.7.0).

Characters outside the supported repertoire or absent from the face share one
replacement glyph (U+FFFD, or `?` if unavailable). Different unsupported
characters alias the same pixels. Spaces normally have an advance without an
atlas rectangle. CRLF and CR normalize to LF; tabs become four spaces. Other
control characters are rejected. Commands constructed directly must already
be normalized and contain only characters prepared in their font resource.

This is character-based horizontal text, with no kerning, shaping, ligatures,
bidi, general combining-sequence layout, rich text, or color emoji. Font
coverage does not imply correct layout for every Unicode string. A future
shaped-run representation will need glyph IDs and positioned advances.

## Worker batches and ownership

Painters append commands to the shared builder and collect a union of required
characters per face/size/scale. After all painters finish, `finish_with_text`
uses the worker's `TextService` to prepare missing glyphs once in deterministic
order and replace each changed configuration at most once. Unchanged
configurations reuse their resource handle; alias-only additions also reuse
atlas pixels. `finish` uses a temporary service for standalone consumers.

Fontdue 0.9.4 parses and rasterizes individual glyphs. Etagere 0.2.15 allocates
padded rectangles. Atlas preparation copies coverage once for a changed batch,
retains existing coordinates when appending, and grows/re-packs geometrically
when needed. Existing glyphs are copied, not rasterized again during growth.

Defaults: 128-square initial atlas, 2048-square maximum, one-pixel padding,
physical font size 1–256 pixels, scale 0.1–16, at most 4096 character aliases,
16 cached configurations, and 32 MiB of cached coverage. `TextLimits` configures
standalone services. Least-recently-used eviction releases cache ownership;
retained outputs may keep snapshots alive beyond these limits. Keys retain face
ownership, preventing pointer reuse from matching an unrelated font.

Each configuration commits atomically. If finalization fails, no partial display
list is returned. Successful earlier configuration updates can remain cached;
the UI preserves its last published output, revision, and geometry. The
application keeps the cache in a worker-local `RefCell` so finalization can
mutate it while renderers read application data; this does not share state
between threads.

## GUI rasterization

Atlas rectangles are physical pixels. Advances, offsets, and line metrics are
logical pixels; offsets use downward-positive coordinates from the baseline.
The GUI reads prepared glyphs, snaps bitmap origins to physical pixel
boundaries, preserves fractional pen advances, and applies nested clips.
Coverage blends foreground and destination with rounded 8-bit interpolation
in encoded RGB, not linear-light or LCD subpixel blending.

Different DPI scales use different font configurations and atlas snapshots.
During a scale transition, a retained output may safely resample coverage until
the matching worker output arrives. Normal rendering rasterizes at the target
physical size rather than enlarging low-resolution glyphs. The GUI performs no
font parsing, rasterization, or atlas allocation.

See
[DR-007](<../../../../docs/decisions/DR-007 Prepare character atlases on the worker.md>)
for the decision and alternatives.

The image, font, and atlas handles use
[shared resource infrastructure](Resources.md). Their equality compares snapshot
identity; pixel and metric comparisons are explicit.
