# GUI renderer plugins

The application worker creates immutable `DisplayList`s. Component painters
produce commands; GUI renderers execute and present them. GPU textures, devices,
and surfaces stay on the GUI thread. The engine does not depend on femtovg.

`host::run` selects Auto. `host::run_with_renderer` selects Auto, Software, or
Femtovg. Auto tries GPU initialization, logs a failed attempt, and falls back to
software for that window. Auto also chooses software when the available adapter
is a CPU implementation. Explicit Femtovg selection returns failure instead.
`host::run_with_factory` accepts a custom `RendererFactory`; its `create` method
receives an `Arc<Window>` and returns one `Box<dyn Renderer>` per window.
Renderers have no Send/Sync requirement.

## Contract

`resize` receives physical dimensions and a logical-to-physical scale.
Zero-sized windows do not render. `render` consumes borrowed commands and
returns Presented or Skipped. The host advances its input revision only after
Presented; Skipped retains the previous revision and retries after at least 33
ms. Errors do not advance the revision. Suspension releases native surfaces,
clears the presented revision, and cancels retries/animation deadlines. Resume
recreates the surface and requests an updated worker presentation.

Femtovg shares a GPU device and queue between windows created by its built-in
factory. Each window owns its canvas, surface, and texture cache. Lost/outdated
surfaces are reconfigured; timeouts/occlusion skip with bounded retries. Device
loss and GPU errors are returned on rendering. Fatal native errors currently
stop the host; runtime backend switching is not implemented.

## Text and images

Both implementations use worker `FontResource` snapshots. Shared glyph placement
handles baseline offsets, advances, spaces/newlines, physical pixel snapping,
and atlas resampling during DPI transitions. Femtovg's font layout is disabled:
coverage bytes become Gray8 textures and custom glyph quads sample the red
channel, tinted by the command color. Nearest sampling matches the CPU painter.
A tiny texel bias prevents boundary roundoff selecting an adjacent coverage
column. Images upload RGB converted to RGBA; the transparent color gets zero
alpha. Resources are not converted on every frame.

Solid rectangle bounds snap outward to match CPU touched-pixel coverage, and
strokes remain inside their rectangle. Nested clips intersect and restore.
Subpixel clip edges can differ from software due to GPU coverage; screenshots
need explicit tolerances. A non-sRGB surface format is preferred to match
encoded RGB blending. On surfaces that only support sRGB formats, blending can
differ.

## Cache lifetime

Caches use snapshot allocation identities with retained Weak references, never
frame-local indices. A Weak reserves its allocation address; dead entries are
pruned without keeping payloads alive. Retained snapshots remain immutable and
can be reuploaded after eviction. Old outputs continue to reference old atlases.

Each window has a 64 MiB texture budget with least-recently-used eviction. A
frame's working set can exceed the budget until submission; trimming follows.
Weak cleanup also removes dead resources. Image handles are released on eviction
and canvas drop; in-flight GPU commands retain the underlying textures.
`FemtovgRenderer::cache_stats` reports cumulative uploads, resident texture
count, and source-format texture bytes. Driver overhead and surface memory are
excluded.

## Validation and profiling

Run ordinary repository checks with `./n check`. GPU tests attempt an adapter
and print an unavailable reason when none exists. Require validation explicitly:

```sh
PIXUI_REQUIRE_GPU=1 ./t cargo-nextest nextest run -p pixui-gui renderer::scene::tests
```

The release profiling fixture is intentionally ignored in normal runs:

```sh
PIXUI_REQUIRE_GPU=1 ./t cargo-nextest nextest run --release -p pixui-gui \
  profile_release_renderer_phases --run-ignored only --success-output immediate
```

This measures command translation/upload and CPU submission separately from CPU
rasterization, using an offscreen target. It does not measure worker traversal,
compositor latency, or end-to-end GPU execution time. For actual GUI comparisons
use identical window sizes, DPI, animation rates, and release builds. GPU
drawing does not remove worker preparation or the host's finite mailbox polling.
