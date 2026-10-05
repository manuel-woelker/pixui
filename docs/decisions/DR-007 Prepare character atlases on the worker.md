# DR-007: Prepare character atlases on the worker

- Status: Accepted
- Date: 2026-10-05

## Decision

Send compact text commands containing strings and local font-resource indices.
Prepare missing glyphs after painting on the application worker, using fontdue
and etagere, and share immutable grayscale atlas snapshots through `Arc`.
Start with one atlas per face/size/scale and character-based horizontal layout.
Keep our resource types independent of those libraries, with concrete internal
helpers rather than a backend plugin interface.

Embed static Geist Regular TTF downloaded by tool-tool. Limit initial rendering
to Latin characters, with a shared replacement glyph. Use baseline origins,
no kerning or automatic wrapping, and real advances and line metrics.

## Context

Pixui's worker owns application and UI state and produces draw lists for
multiple independent windows. Animated components repaint frequently. The old
8-by-8 bitmap font lacked scalable antialiasing and real text metrics. Sending
one positioned draw command per glyph would enlarge each frame, and sending
atlas pixels again each frame would waste work and bandwidth.

## Rationale

One string and font index keep commands compact. Collecting missing characters
across all painters avoids repeated preparation and makes painter ordering
irrelevant. Immutable snapshots keep retained outputs internally consistent
when glyph maps grow or atlases repack; new consumers need no upload history.
Unchanged frames share exact resources across windows and themes.

Fontdue supplies straightforward metrics and coverage. Etagere supplies
rectangle allocation. Keeping their types private makes either replaceable
without prematurely inventing a plugin contract. Tool-tool pins and caches
font downloads; embedding TTF bytes makes executable behavior reproducible
without depending on installed system fonts or runtime cache paths.

## Consequences

Coverage blending adds antialiasing to the GUI CPU renderer. Cache keys include
face identity, size, and scale; color and position do not create new atlases.
Bounded caches and single-atlas limits produce descriptive errors and preserve
the last good output. Changed drawable glyph batches copy the immutable atlas
once; spare capacity reduces repacking, not snapshot copying. Retained frames
can keep evicted resources alive beyond the cache budget.

This initial representation does not support shaping, bidi, ligatures, kerning,
or general combining sequences. A shaped-run representation will require
additional glyph-ID and positioning data. Different DPI settings need separate
resources; retained outputs temporarily resample during transitions. Static
Geist's source TTF is not byte-subsetted; the runtime repertoire is Latin.
Distributions must include the font license and copyright notice.

## Considered alternatives

- **Rasterize on the GUI thread:** simpler worker resources, but introduces font
  parsing/cache work into native callbacks and duplicates it across clients.
- **Send positioned glyphs:** supports shaping naturally, but increases frame
  size for repeated text. Add when actual shaping requirements justify it.
- **Keep fixed bitmap cells:** small implementation, but poor scaling and
  incorrect advances for proportional text.
- **GPU upload IDs and explicit release messages:** can reduce transfer across
  processes, but adds ordering and lifetime protocols. Arc snapshots suffice
  for the current in-process renderer.
- **Multiple atlas pages:** avoids a hard single-atlas capacity limit, but adds
  page identities and sampling branches before demonstrated need.
- **WOFF2:** smaller downloaded assets, but needs decompression before the TTF
  parser. Static TTF is directly supported and compressed release downloads
  already reduce acquisition size.
- **System fonts or a subsetting pipeline:** broaden discovery or shrink assets,
  but add environment variability or build tools. Pin the official static face
  and prepare only needed Latin glyphs initially.
- **Backend traits/plugins now:** speculative boundaries without a second
  implementation. Use named concrete modules and extract an interface later.

## References

- [Text API and limits](../../crates/engine/src/ui/Text.md)
- [Fontdue](https://docs.rs/fontdue/0.9.4/fontdue/struct.Font.html)
- [Etagere](https://docs.rs/etagere/0.2.15/etagere/struct.AtlasAllocator.html)
- [Geist 1.7.0](https://github.com/vercel/geist-font/releases/tag/1.7.0)
- [Geist license](../licenses/Geist%20OFL.txt)
