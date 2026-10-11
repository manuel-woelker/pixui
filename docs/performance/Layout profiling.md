# Showcase layout profiling

## Workload and reproduction

Measured on 2026-10-11 using the Layout page: 36 project cards, 216 task rows,
617 components and 528 containers, at an English/light 800 by 600 viewport.
The headless profiler excludes startup and 20 warmup frames. Each sample
requests a complete worker redraw; there is no native renderer, hot reload or
performance overlay. All loop occurrences are laid out and painted, including
offscreen content.

Before retained layout was implemented, a run with all experimental changes and
temporary timers removed measured **12.080 ms median layout**,
**12.752 ms p95**, and **13.620 ms median worker stage total** over 300 warmed
frames. It still made 22,256 measurements per frame. The instrumented
experiments below have additional timer overhead and run-to-run variation; their
percentages are exploratory comparisons rather than guaranteed speedups.

```bash
./t cargo run --release -p pixui-example-showcase --bin profile_showcase_layout -- --frames 300
```

Use `--width 1200 --height 800` for another viewport. Omit `--release` to
measure debug builds. The profiler reports worker stage medians/p95s, component
and container counts, and painter measurement calls. Layout includes painter
callbacks and geometry extraction. Timings depend on the machine and workload.

## Where time goes

Temporary timers around measurement callbacks and geometry extraction split
the release layout stage as follows:

| Work | Median time | Approximate share |
| --- | ---: | ---: |
| Solver excluding painter callbacks | 11.298 ms | 81% |
| Painter measurement callbacks | 2.547 ms | 18% |
| Geometry and clip extraction | 0.046 ms | <1% |
| Complete layout stage | 13.905 ms | 100% |

Individual medians do not necessarily sum exactly. Timers add overhead to this
instrumented run, particularly with 22,256 callbacks per frame. The temporary
instrumentation was removed after profiling.

The main cost is now solving the nested hierarchy, rather than glyph metrics.
There are approximately 36 measurement probes per component. Nested Grid
intrinsic sizing and Flex layout probe different allocations; some identical
measurement constraints also recur. CPU sampling in a build with the engine
and Taffy optimized showed Flex solving, child traversal, layout-cache handling
and leaf computation among the main hotspots.

Every normal render currently constructs a fresh Taffy tree. Taffy caches work
within that solve, but its cached results are discarded before the next frame.
This makes unchanged animation redraws pay the full initial-layout cost.

## Experiments

These were temporary prototypes, measured separately in release builds at the
same viewport. They were restored after profiling; the retained changes are
the reproducible profiler and this report. The implemented cache is described
below.

| Experiment | Layout median | Measurements/frame | Tradeoff |
| --- | ---: | ---: | --- |
| Original hierarchy | 13.905 ms | 22,256 | Full solve every frame |
| Cache identical painter calls within one frame | 12.772 ms | 9,543 actual painter calls; 22,256 solver probes | About 8% faster; solver work remains |
| Remove one wrapper around each task-name label | 10.200 ms | 19,016 | About 27% faster; 216 fewer containers |
| Set task-row height to 32 logical pixels | 9.962 ms | 16,136 | About 28% faster; changes row geometry |

The wrapper experiment put Flex growth directly on the task-name component,
instead of on a single-child Flex column around it. It kept the component
count and content. Geometry, focus, clips and hit testing should be compared
before adopting this change; these timings alone do not establish equivalence.

Fixed row heights reduce intrinsic height calculations, but require a deliberate
design that accommodates theme fonts, translations and accessibility sizing.
Hardcoding dimensions is not a general substitute for intrinsic layout.

Re-solving the **exact unchanged Taffy tree** immediately after its first solve
took a median **0.175 microseconds**, with no measurement callbacks. This was
a separate cache-hit experiment over 50 warmed frames. It measured only the
solver call: preparation, reconciliation, construction, extraction and painting
were excluded. It is evidence for retaining solver caches, not an estimate of
the complete optimized frame cost.

## Recommended solutions

1. **Remove unnecessary containers.** The task-name wrapper has no padding,
   gap or additional children; assign growth directly to its label. Verify
   geometry and hit testing in both languages and at narrow/wide viewports.
   This is the simplest improvement to initial and invalidated layouts.
2. **Retain the Taffy tree per UI instance.** Reconcile physical occurrences
   using their stable component paths, update changed styles/children, and mark
   nodes dirty when their measurement inputs change. Unchanged redraws can use
   Taffy's existing cache; this addresses the largest recurring cost.
3. **Define invalidation before skipping layout.** Props, component state,
   painter/style replacement, font metrics, locale, DPI, viewport dimensions,
   image dimensions, loop membership and match selection can affect geometry.
   Custom painters may depend on any measurement input. Conservatively
   invalidate when dependencies cannot be checked; do not infer that an
   animation or hover update is always geometry-neutral.
4. **Cache exact measurement results within a solve if still useful.** Include
   the occurrence and all known/available constraints. Prepared props, state,
   styles and settings are constant during that solve, and measurement is
   documented as pure. This needs no cross-frame invalidation, but only saved
   about 8% here. Caching intrinsic sizes across different constraints needs an
   explicit painter contract because custom painters may wrap or adapt content.
5. **Consider virtualization for much larger lists.** The current stress demo
   intentionally processes offscreen cards. Painting culling alone does not
   remove solver work. Virtualized rows need estimated/measured heights and
   correct scroll, focus and hit-testing behavior; defer this complexity until
   real list sizes justify it.

Before implementing persistent caching, add regression coverage for changes to
text, themes, locales, viewport/DPI, image dimensions, keyed loops, match arms,
and shared state across windows. Profile unchanged redraws and invalidated
layouts separately: a cache-hit benchmark cannot establish initial-layout
performance.

## Implemented retained layout

Each application-managed UI instance now owns its Taffy tree. The engine
compares the physical hierarchy and layout styles, then validates all distinct
measurement constraints from the last full solve against current prepared
inputs. This supports arbitrary pure, deterministic custom painters and
constraint-sensitive measurements without adding equality bounds to props or
state. Equal measurement answers reuse the solver cache; changed answers or
styles conservatively invalidate all nodes. Structural changes rebuild the
tree. Probe history resets on invalidation, and measurement failures discard the
cache. Geometry, scrolling, clips and action targets are extracted anew.

Same release workload, 300 warmed frames, with no temporary instrumentation:

| Metric | Before | Retained tree |
| --- | ---: | ---: |
| Layout median | 12.080 ms | 0.765 ms |
| Layout p95 | 12.752 ms | 0.927 ms |
| Worker stage total median | 13.620 ms | 2.225 ms |
| Painter measurements per warmed frame | 22,256 | 9,543 |

Warm layout is approximately **94% lower**, and total worker stage time is
approximately **84% lower**. The remaining measurement calls validate cached
answers; Taffy itself does not remeasure the unchanged tree. The first render
measured 16.159 ms layout and 22,256 calls; initial and invalidated layouts
still require full solving, so the warmed-frame reduction does not describe
them. That initial timing is one startup sample, not a cold-layout distribution.

Further optimization could reduce validation with an explicit painter
measurement-dependency contract, or invalidate individual subtrees. Both need
additional correctness work; the current conservative cache already removes
most repeated solver cost.
