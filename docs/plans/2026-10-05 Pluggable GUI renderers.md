# Pluggable GUI renderers

Status: proposed; repository and upstream API review complete. GPU prototype,
visual verification, and performance measurements remain implementation gates.

## Goal

Make native display-list execution replaceable. Add SoftwareRenderer using the
current CPU painter and softbuffer, and FemtovgRenderer using its wgpu backend.
Keep worker-produced image snapshots, glyph coverage atlases, and compact text
commands. GPU handles remain on the GUI thread.

## Design

Place the public contract and concrete implementations in named modules under
`crates/gui/src/renderer/`; module roots contain only module declarations.
The GUI host owns input, scheduling, windows, and presented revisions. Each
renderer owns its surface and drawing resources for one window.

Illustrative contract, to finalize during implementation:

```rust,ignore
trait Renderer {
    fn resize(&mut self, width: u32, height: u32, scale: f32) -> PixuiResult<()>;
    fn render(&mut self, display: &DisplayList) -> PixuiResult<RenderOutcome>;
    fn suspend(&mut self);
    fn resume(&mut self) -> PixuiResult<()>;
}

enum RenderOutcome {
    Presented,
    Skipped,
}

trait RendererFactory {
    fn create(&mut self, window: Arc<Window>) -> PixuiResult<Box<dyn Renderer>>;
}
```

The factory runs on the GUI thread and can initialize shared GPU device/queue
state once. Do not require Send or Sync on renderers. Cache textures per window
initially; sharing devices does not imply sharing canvas-local image handles.
Support custom factories through a host entry point, plus convenient built-in
selection: Auto, Software, Femtovg. Preserve the existing `run` entry point as
Auto. Auto tries GPU initialization and reports a software fallback; explicit
Femtovg selection reports initialization failure instead of silently falling
back. Avoid runtime backend switching in this first version.

Zero-sized windows skip rendering and surface configuration. Update the host's
presented revision only for Presented, retaining the prior revision on skip or
failure. Recover lost/outdated surfaces by reconfiguration; retry timeouts on a
bounded schedule without a busy loop. Report unrecoverable device or allocation
failures. Suspend releases surface resources and cancels animation scheduling;
resume restores them and requests current presentation. GPU presentation success
means successful submission/present, not waiting for the GPU to finish.

## Display-list translation

| Command | Software | Femtovg |
|---|---|---|
| FillRect / StrokeRect | Existing pixel implementation | Rectangle path fill/stroke |
| DrawImage | Existing sampled image | Cached texture and image paint |
| DrawText | Existing atlas coverage blending | Worker atlas and glyph quads |
| PushClip / PopClip | Existing intersection stack | Save, intersect scissor, restore |

Keep painter order. Stroke placement, clipping, logical coordinates, physical
window dimensions, and DPI must follow existing behavior. Share glyph-position
calculation where it avoids divergence: baseline, offsets, advances, newlines,
spaces, snapping, and old-atlas resampling during DPI transitions.

No backend font loading, shaping, or glyph rasterization. Upload coverage using
femtovg's supported mask format and emit `draw_glyph_commands`; verify sampling
and tinting first. If necessary, use a one-time RGBA representation appropriate
for that path. Use opaque RGB images converted to RGBA on upload; pixels
matching the optional transparent key receive zero alpha. Choose filtering
explicitly to match existing image sampling and avoid neighboring glyph bleed.
Account for premultiplication and sRGB blending; GPU and CPU edge pixels may
differ.

Validate the list before drawing or changing presentation state. GPU rendering
must never allocate/read back an entire CPU framebuffer during ordinary frames.
Keep the existing pure CPU `painter::paint` API for deterministic tests and
exports.

## Resource cache

Use typed ResourceIdentity keys for immutable image and font snapshots; frame
indices only address resources in that particular display list. Convert/upload
once per resident identity, never once per frame. New atlas snapshots remain
separate from old retained versions.

Each cache entry holds a weak snapshot reference alongside its key. Check weak
liveness before reuse and prune dead entries during frames. A retained Weak
keeps the allocation reserved; update ResourceIdentity documentation to explain
this safe cache pattern. Release femtovg image handles on eviction, suspend
teardown where appropriate, and renderer drop, with correct GPU submission
ordering.

Add a documented byte budget and LRU eviction: worker font caches and retained
outputs can keep snapshots alive long after this renderer needs them. Weak
cleanup alone cannot bound GPU memory. Resources needed by the current frame
are pinned until submission. If a frame exceeds the budget, allow its working
set temporarily and trim afterward; reject unsupported texture dimensions and
upload sizes with useful errors. Record upload/cache counters for verification.

## Implementation checklist

- [ ] Record a software baseline in a release build: both todo windows, fixed
      sizes/DPI, idle and animated CPU usage, and frame duration.
- [ ] Pin compatible femtovg/wgpu versions after a small offscreen prototype
      proves colored coverage-atlas glyphs, transparent images, and nested
      clips. Keep font layout features disabled if the verified public API
      allows it.
- [ ] Extract Renderer/RendererFactory and wrap current software presentation.
      Add host selection, suspend/resume, and accurate presentation outcomes.
- [ ] Implement femtovg surface setup, resize/recovery, command translation,
      worker-atlas text, cache identity, upload conversion, budget, and cleanup.
- [ ] Add todo `--renderer auto|software|femtovg` selection, preserving
      `--custom-painter`, two-window behavior, and animation scheduling.
- [ ] Make Auto prefer GPU after compatibility and measurements pass; keep
      Software explicitly selectable and document automatic fallback.
- [ ] Update Architecture.md, GUI/example docs, and a renderer decision record.
      Update the architecture diagram if its represented contracts change.
- [ ] Run `./n check` after each unit and fix introduced failures.

## Verification checklist

- [ ] Fake renderers verify factory lifecycle, resize/DPI, suspend/resume,
      Presented/Skipped/error revision handling, and per-window independence.
- [ ] Existing CPU painter tests stay deterministic. Exercise all commands,
      nested/empty clips, fractional coordinates, and strokes on GPU offscreen.
- [ ] Compare readback fixtures with tolerances for antialiasing/color
      differences: colored text, multiple glyphs, spaces/newlines, DPI, key
      transparency, overlapping images and text, clipping, and retained old
      atlas snapshots.
- [ ] Confirm repeated outputs upload each resident snapshot once; changed
      indices reuse it; new identities upload; cleanup and budget eviction work.
- [ ] GPU tests report unavailable adapters explicitly. Require an actual GPU
      validation run before declaring the backend verified; software-only CI
      success does not establish GPU correctness.
- [ ] Manually inspect both todo themes/locales, resizing/DPI, focus, scrolling,
      filtering, stale inputs, animations, and closing either window.
- [ ] Compare release CPU usage and frame durations against the baseline, with
      identical content and animation rate. Separate worker preparation, texture
      upload, submission, and presentation costs; report
      hardware/driver/backend.
- [ ] Run final `./n check`, record results, then move this plan to completed.

## Validation findings and open questions

Reviewed the current host, CPU painter, resource identity, and display-list API.
The worker boundary already supports both renderers without command-format
changes. The host currently owns softbuffer surfaces; moving these behind the
contract also requires replacing its surface-presence checks with renderer
readiness. The 16 ms mailbox polling and 33 ms animation requests remain
scheduling costs; this plan does not claim GPU drawing eliminates them or worker
tree traversal.

Upstream documentation confirms a wgpu renderer and custom glyph command API:
[WGPURenderer](https://docs.rs/femtovg/latest/femtovg/renderer/struct.WGPURenderer.html),
[Canvas](https://docs.rs/femtovg/latest/femtovg/struct.Canvas.html#method.draw_glyph_commands).
These support architectural feasibility; exact mask-channel/blending behavior,
feature availability, and dependency versions must be confirmed in the pinned
prototype. No GPU prototype or performance measurement has been performed yet.

Open implementation choices: compatible dependency versions, mask upload format,
initial GPU cache budget, and acceptable visual comparison tolerances. Decide
from the prototype and existing engine limits. Shared texture caches, additional
backends, dirty-region repainting, and GPU-generated comets are separate work.
