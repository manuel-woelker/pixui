# DR-006: Share immutable images in indexed display lists

- Status: Accepted
- Date: 2026-10-05

## Decision

Represent runtime RGB images as immutable snapshots backed by `Arc`, with width,
height, and optional exact color-key transparency. Each display list owns an
image table; drawing commands reference its indices. One shared builder
deduplicates snapshots by allocation identity and accepts commands directly from
all painters.

Prepare component props and updates before painting at final row positions.
Paint contexts translate local coordinates and protect renderer clips. Use
nearest-neighbor image sampling initially. Add optional per-output redraw
requests for animation, with one outstanding deadline per window.

## Rationale

Sharing avoids repeated source copies and releases snapshots through ordinary
ownership. Complete output remains safe when pending frames are dropped and when
windows retain older frames. An indexed table avoids per-command ownership and
supports future backend resource preparation. Direct insertion avoids local list
merges and image-index remapping.

A small animation API exercises runtime image creation without introducing an
unconditional render loop or another timer thread. Geometry-compatible visual
redraws keep presented clicks usable while preserving existing action bindings.

## Context

The worker sends display lists to a CPU-rendered native GUI through
latest-output mailboxes. Components originally produced local command buffers.
Runtime images must support changing pixels and multiple windows without
transmitting copies of unchanged source arrays. The todo GUI needs an animated
custom image component.

## Consequences

- Images can be created from ordinary RGB vectors without an asset pipeline.
- Published images cannot be mutated; replacement allocates a new version.
- Color-key transparency has no partial alpha and reserves one RGB color.
- Snapshot identity equality avoids comparing pixel arrays during output checks.
- Every frame owns its resources and can be drawn without resource history.
- Retained outputs keep older allocations alive; a per-image limit is not a
  total memory budget. CPU sampling and framebuffer copying still occur on
  redraw.
- All component preparation precedes painting, changing the previous interleaved
  lifecycle. State is reborrowed without clones or another expression
  evaluation.
- Failed rendering discards the shared builder and stops animation scheduling
  until another invalidation retries. Earlier updates are not rolled back.
- Visual redraws preserve actions only while content and geometry remain
  compatible.

## Considered alternatives

### Copy pixels into every frame or command

Rejected because repeated use and multiple windows would require large source
copies. Shared allocations provide stable retained frames with smaller overhead.

### Use a GUI cache with explicit acquire and release messages

Deferred because reliable version updates, dropped-frame recovery, and cleanup
would add a protocol without a prepared-resource backend needing it yet.

### Keep mutable shared pixels behind a lock

Rejected because writes could change retained output or block rasterization.
Immutable versions keep frame contents coherent without locks.

### Keep separate component display lists

Rejected because image tables would need merging and command-index remapping.
Preparing first allows direct shared insertion at known row coordinates.

### Use a continuous animation loop or dispatch dummy domain actions

Rejected because idle UIs would render unnecessarily or animation would
invalidate content and interaction. Explicit visual redraw requests preserve
ownership and use the existing nonblocking queue.

### Add alpha blending, a GPU cache, or buffer pooling immediately

Deferred because RGB color keys and small replacement snapshots meet the current
requirements. Measure real workloads before adding resource lifecycle
complexity.
