# DR-009 Plug GUI renderers into a shared display list contract

- Status: Accepted
- Date: 2026-10-05

## Decision

Execute display lists through GUI-thread Renderer plugins. Provide software
presentation and femtovg's wgpu backend, with Auto preferring GPU initialization
and a reported software fallback. Keep image and glyph-atlas snapshots generated
on the worker; backend font layout is disabled.

## Context

The first native host rasterizes complete windows on the CPU. Animated
components require frequent redraws. GPU execution should be available without
coupling application state, component painters, or text layout to a graphics
library.

## Rationale

The existing owned display list is already a backend boundary. Femtovg provides
2D drawing and custom atlas glyph commands without requiring handwritten GPU
pipelines. Software remains useful for deterministic tests and environments
without GPU presentation. Factories permit other implementations without
introducing backend handles into worker state.

## Consequences

Each window owns its renderer and surface. GPU resource caches need identity,
cleanup, and residency limits. Only successful presentation advances the input
revision. GPU sampling and clip-edge coverage can differ from software; tests
allow documented tolerances. GPU rendering reduces CPU rasterization work, but
small release workloads may have similar or higher total CPU usage from driver
submission. It does not eliminate animation preparation or host polling.

## Considered alternatives

- Keep software only: simplest dependency graph, but full-window rasterization
  scales with physical pixels and redraw frequency.
- Write custom wgpu pipelines: gives tighter control, but adds shader, batching,
  and antialiasing implementation before a demonstrated need.
- Use backend-managed fonts: convenient, but duplicates worker font layout and
  can diverge from its metrics and published glyph snapshots.
- Add Vello and Skia immediately: increases build and maintenance scope. Two
  implementations exercise the contract; add others for concrete requirements.
- Share GPU textures across all windows: could reduce duplicate uploads, but
  canvas-local handles and lifetime coordination add complexity. Share the
  device/queue first and retain independent bounded caches.
