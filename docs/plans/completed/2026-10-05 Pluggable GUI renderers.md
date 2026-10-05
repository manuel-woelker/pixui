# Pluggable GUI renderers

Status: completed. Software and femtovg/wgpu presentation plugins are
implemented, with hardware GPU fixtures, manual verification, and release
measurements.

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

- [x] Record a software baseline in a release build: both todo windows, fixed
      sizes/DPI, idle and animated CPU usage, and frame duration.
- [x] Pin compatible femtovg/wgpu versions after a small offscreen prototype
      proves colored coverage-atlas glyphs, transparent images, and nested
      clips. Keep font layout features disabled if the verified public API
      allows it.
- [x] Extract Renderer/RendererFactory and wrap current software presentation.
      Add host selection, suspend/resume, and accurate presentation outcomes.
- [x] Implement femtovg surface setup, resize/recovery, command translation,
      worker-atlas text, cache identity, upload conversion, budget, and cleanup.
- [x] Add todo `--renderer auto|software|femtovg` selection, preserving
      `--custom-painter`, two-window behavior, and animation scheduling.
- [x] Make Auto prefer GPU after compatibility and measurements pass; keep
      Software explicitly selectable and document automatic fallback.
- [x] Update Architecture.md, GUI/example docs, and a renderer decision record.
      Update the architecture diagram if its represented contracts change.
- [x] Run `./n check` after each unit and fix introduced failures.

## Verification checklist

- [x] Fake renderers verify presentation lifecycle, resize/DPI, suspend/resume,
      Presented/Skipped/error revision handling, and per-window independence.
      Native factory creation is exercised by the manually verified two-window
      GUI.
- [x] Existing CPU painter tests stay deterministic. Exercise all commands,
      nested/empty clips, fractional coordinates, and strokes on GPU offscreen.
- [x] Compare readback fixtures with tolerances for antialiasing/color
      differences: colored text, multiple glyphs, spaces/newlines, DPI, key
      transparency, overlapping images and text, clipping, and retained old
      atlas snapshots.
- [x] Confirm repeated outputs upload each resident snapshot once; changed
      indices reuse it; new identities upload; cleanup and budget eviction work.
- [x] GPU tests report unavailable adapters explicitly. Require an actual GPU
      validation run before declaring the backend verified; software-only CI
      success does not establish GPU correctness.
- [x] Manually inspect both todo themes/locales, resizing/DPI, focus, scrolling,
      filtering, stale inputs, animations, and closing either window.
- [x] Compare release CPU usage and frame durations against the baseline, with
      identical content and animation rate. Separate worker preparation, texture
      upload, submission, and presentation costs; report
      hardware/driver/backend. Worker traversal and compositor latency are
      unchanged and were not separately instrumented; limits are recorded below.
- [x] Run final `./n check`, record results, then move this plan to completed.

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
prototype. The implementation validates these APIs with offscreen fixtures and
native presentation. Gray8 mask uploads work with custom glyph commands.

Resolved choices: femtovg 0.27.0 with wgpu 30.0.1, textlayout disabled, Gray8
coverage masks, a 64 MiB per-window LRU budget, and software comparison allowing
up to 20 subpixel edge differences with a 3-channel-value rounding tolerance.
Shared texture caches, additional backends, dirty-region repainting, and
GPU-generated comets are separate work.

## Implementation and validation results

- Plan committed first as `28f13ad`. Public contracts and backends are named
  modules in `crates/gui/src/renderer/`; host entry points support built-in
  selection and custom factories. Auto prefers hardware GPU initialization;
  CPU adapters and initialization errors fall back to software with a message.
- Software presentation retains the pure CPU painter API. Shared glyph
  positioning preserves advances, offsets, snapping, and newline behavior.
  Femtovg disables font layout and uses the worker's coverage snapshots through
  custom quads.
- Actual GPU tests used NVIDIA GeForce RTX 2070 SUPER, Vulkan, NVIDIA driver
  610.43.02. Readbacks cover all commands, colored coverage, transparency,
  nested and empty clips, DPI 1/1.5/2, index changes, retained atlas versions,
  invalid lists, weak cleanup, oversized working sets, and LRU
  eviction/reupload.
- Fake renderer tests exercise presentation outcomes, bounded retry state,
  zero dimensions, scale/resize calls, suspension/resume, failure retention,
  and independent revision state. Existing CPU tests pass unchanged.
- User confirmed the native two-window GPU example looks correct. Architecture
  documentation, diagram source, example usage, and DR-009 are updated.
- Added `--freeze-animation` for repeatable idle measurements through the
  existing master timestamp; it does not alter animation code.
- Twelve-second release runs with the two default windows (640x480 and 420x640,
  desktop scale, unchanged animation rate): original software baseline 3% CPU;
  integrated software animated 4% (0.50 s user / 0.01 s system), femtovg
  animated 5% (0.31 s user / 0.38 s system). Frozen software 0% (0.01 / 0.01 s),
  frozen femtovg 3% (0.06 / 0.35 s). These coarse whole-process measurements
  include startup and driver initialization; they do not establish steady idle
  usage or a speed advantage for this tiny UI.
- Explicit warmed release fixture, 100 offscreen 640x480 frames: command
  translation/cache lookup 0.671 ms total, CPU GPU-command encoding/submission
  7.835 ms total, software rasterization 3.800 ms total. Only two textures
  uploaded across the run. The fixture is deliberately small; GPU command
  overhead exceeds the simple software rasterizer here. Measurements exclude
  worker traversal and compositor latency, and do not separately measure GPU
  execution. The ignored profiling test records phases without introducing
  timing assertions into CI.
- `./n check` passes, including formatting, compilation, clippy, nextest, and
  doctests. GPU adapter absence is explicitly reported; PIXUI_REQUIRE_GPU makes
  an unavailable adapter fail validation. Hardware validation was run
  separately.
- Device/validation errors are reported through the renderer; surfaces recover
  from lost/outdated acquisition, with timeouts/occlusion requesting bounded
  retries. Real device-loss injection and other desktop platforms remain future
  platform validation, not claimed by these tests.
