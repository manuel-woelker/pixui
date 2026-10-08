# Component layout

Status: proposed; review remaining layout scope and invalidation questions
before implementation.

## Goal

Replace constant-height flattened rows with hierarchical component layout using
Taffy. Compute on the application worker and retain final positions, dimensions,
clips, and hit geometry per `UiInstance`. A definition can appear in windows
with different sizes, languages, and display scales without sharing geometry.

## Recommended ownership

| Responsibility | Owner | Reason |
| --- | --- | --- |
| Semantic props, behavior, prepared resources | Component | These remain independent of the visual implementation. |
| Intrinsic visual measurement | Painter | Text fonts, icon sizes, control chrome, and theme metrics must agree with painting. |
| Explicit sizes, min/max, margins, padding, gaps, flex rules | Live definition and containers | These express composition and application intent. |
| Final size and position | Layout engine | Parent constraints and siblings determine actual allocation. |
| Published rectangles, clipping, effective scrolling, hit testing | UiInstance | Geometry depends on its viewport and presentation. |

A component supplies semantic content and prepared data, for example image
pixel dimensions. Its painter turns that into a natural logical size. The
painter requests space through measurement; it does not choose its final
position or unilaterally override the allocated size during painting.

Do not add `Component::measure` initially. A button painted with another font or
checkbox style can need different space with exactly the same component props.
Keep measurement alongside `Painter::paint`, sharing visual metrics/helpers so
padding and font sizes cannot silently diverge.

## Existing integration

`ui/renderer.rs` prepares the live tree, collects leaves into a flat list, and
assigns every component the same height, full available width, and global gap.
`PaintContext` already provides allocated dimensions and translates local draw
commands. `UiInstance::LayoutState` already stores component rectangles, hit
regions, content height, and clamped scrolling for the last successful frame.
Extend this existing ownership rather than introducing shared geometry.

Focus, hover, and requested scrolling are definition-wide by prior decision.
Keep them shared. Each instance derives its own geometry and clamps the shared
scroll request to its own content extent. This work does not move interaction
state into instances or add keyed component identity.

## Taffy integration

Use Taffy in the engine crate. The current documented release is 0.14.0; verify
and pin the chosen stable version and necessary features during implementation.
Enable flexbox, content-size support, and `TaffyTree` as a private adapter.
Enable Grid for the basic tracks and placement subset below.
No CSS parsing, selectors, or renderer/native layout dependency.

Taffy offers both an owned tree and lower-level traits for an existing tree.
Despite the lower-level API's relevance to UI frameworks, start with a temporary
owned layout tree: our live tree contains abstract loops and matches and is not
itself the physical layout tree. This minimizes cache and reconciliation work.
Rebuild the temporary tree per full render initially; store the committed
geometry on the instance. Consider persistent node IDs/caches after measuring.

The 0.14 measurement API takes layout inputs and returns layout output;
integrate through its leaf-layout helper rather than copying an older
four-argument measurement example. Keep Taffy errors, IDs, and callback
conventions internal. Verify enabled-feature Send constraints before putting a
tree into any Send object; construct/use it on the worker and transfer only
owned render output.

## Definition and container API

Propose explicit `ContainerPart` with layout style and children, plus per-leaf
layout overrides on `ComponentPart`. Provide convenient row/column constructors.
Keep `CompositePart` as a transparent fragment, preserving existing grouping
semantics. A loop inserts one physical body per item; a match inserts only its
selected arm. Neither introduces a layout box unless its body/arm contains an
explicit container. An unmatched arm contributes no space or gap.

Illustrative shape, not final Rust API:

```rust,ignore
ContainerPart::column()
    .with_gap(8.0)
    .with_padding(12.0)
    .with_children(vec![
        label.with_layout(LayoutStyle::auto()),
        ContainerPart::row()
            .with_gap(8.0)
            .with_children(vec![
                input.with_layout(LayoutStyle::grow(1.0)),
                add_button.with_layout(LayoutStyle::auto()),
            ])
            .into(),
        todos_loop,
    ])
```

Recommended initial style subset: auto/logical lengths/percentages, min/max
size, aspect ratio, margin, padding, row/column, gap, grow/shrink/basis, and
start/center/end/stretch alignment. Validate finite numeric values and
nonnegative dimensions/padding/gaps. Define percentage units unambiguously.
Keep sibling paint order in definition order. Defer absolute
positioning, baseline alignment, z-index, and nested scroll containers.

Prefer a small engine-owned `LayoutStyle` for this supported subset, translated
into Taffy style. This keeps the public contract honest about what drawing and
input support. Avoid wrapping all of Taffy's CSS properties speculatively.
Direct exposure of `taffy::Style` was considered but rejected because of
version coupling and unsupported-feature risks.

Use an implicit root column to preserve existing top-level definitions. Migrate
the examples to explicit containers so nested layout is exercised. Allow a leaf
to override stretch with intrinsic sizing and to request a fixed size for custom
animated content. Unsupported low-level state-factory leaves retain their
existing non-rendering behavior.

## Flex and Grid containers

Use one `ContainerPart`, with a container layout enum selecting its algorithm:

```rust,ignore
enum ContainerLayout {
    Flex(FlexLayout),
    Grid(GridLayout),
}
```

Keep common box constraints in `LayoutStyle`. A container's own layout algorithm
and its placement as an item in its parent are separate settings: a Grid
container can grow inside a Flex row, and a Flex column can span Grid cells.
Neither algorithm belongs in a component or painter. Both use the same leaf
measurement, per-instance geometry, clipping, painting, and hit-testing path.

- **Flex:** one-dimensional composition. `row()` and `column()` are shortcuts
  for Flex direction; expose gap, grow/shrink/basis, main-axis distribution,
  cross-axis alignment, and optional Flex wrapping. Flex wrapping moves entire
  items between lines; it does not require wrapping text inside a label.
- **Grid:** two-dimensional alignment. Recommend a small initial subset:
  explicit columns/rows, auto-sized implicit tracks,
  logical-length/auto/fraction tracks, min/max tracks, row-major automatic
  placement, and explicit child row/column starts and spans. Expose row/column
  gaps and item alignment. Use one-based positive track starts and positive
  spans; omitted placement uses row-major automatic placement. No negative
  indices in the initial API. Fraction tracks divide remaining space after
  fixed tracks and gaps, subject to their minimum constraints. Default fraction
  tracks to a zero minimum; use explicit auto/min-content minima when desired.
  Defer named areas, named lines, subgrid, dense placement, and advanced repeat
  syntax.

Illustrative builder shape, not final Rust API:

```rust,ignore
ContainerPart::grid()
    .with_columns(vec![Track::auto(), Track::fraction(1.0)])
    .with_gap(8.0)
    .with_children(vec![
        name_label.into(),
        name_input.into(),
        enabled_label.into(),
        enabled_checkbox.into(),
    ])
```

This aligns labels and controls across rows without calculating widths in
painters. A responsive toolbar can use Flex instead. Grid track constraints and
child minimum sizes must explicitly allow shrinking where needed; nonwrapping
text should clip rather than unexpectedly force a window-wide overflow.

Transparent fragments, loops, and matches contribute their physical children
as items to either algorithm. Wrap a repeated card's body in an explicit
container when the whole card should occupy one Grid cell. Otherwise each leaf
occupies a separate cell. Filtering automatically placed items repacks the Grid;
explicit placements can intentionally leave empty cells.

Include basic Grid alongside Flex, sharing one adapter and box model. Keep
advanced CSS features out of the API.

## Box, clipping, and style contracts

- Explicit style sizes and min/max constraints describe border boxes. Padding
  is inside that allocation; margins are outside it. Engine-managed borders
  are not part of the initial style API.
- `PaintContext.width/height` and its local origin describe the content box,
  after engine padding. Painter visual insets remain part of measured content.
  Clamp content dimensions to zero when padding consumes the allocation.
- Interactive targets use the border box, including padding and excluding
  margins, intersected with effective ancestor clips and the viewport.
- Containers clip descendants to their border boxes by default. Components
  clip drawing to their content boxes. The viewport always clips. Do not expose
  configurable overflow in this milestone. Apply identical effective clipping
  to painting, hover, and pointer activation.
- Vertical scroll extent includes content and margins within the scrolling
  subtree, including trailing margins. Descendants clipped by a fixed-size
  container do not expand the outer scroll extent beyond that container's box.
- Layout styles are constant definition data initially. Resolve any future
  expression-based sizes or placements into prepared physical nodes, never by
  mutating the loop's shared template for each item.
- Flex items default to no growth, shrink enabled, and auto basis. Intrinsic
  control minimums prevent unwanted collapse; explicitly shrinkable content
  can set zero minimums. Grid items default to automatic placement and stretch.
  Test defaults with long nonwrapping text rather than relying on implicit
  upstream minimum-size behavior.

## Measurement contract

Add a typed read-only `MeasureContext<C>` and `Painter<C>::measure`, dispatched
through the existing checked erased painter adapter. It receives prepared props
and state, presentation settings, known dimensions, and available space. Space
must distinguish definite, min-content, and max-content constraints; do not
represent unconstrained size with a magic float or infinity.

- Return a finite nonnegative natural size; respect axes already fixed by the
  solver. Taffy resolves explicit style and parent constraints.
- Report content size excluding engine-managed style padding/border. Intrinsic
  painter chrome, such as a checkbox glyph and its own visual inset, must be
  included once. Distinguish these in the box-model documentation.
- Measurement may run repeatedly, in any solver-required order, including probes
  for min/max-content. Never mutate component state, dispatch actions, request
  animation, perform I/O, emit commands, or allocate glyph atlas coverage here.
- Provide text metrics without needing a display-list sink. Share normalization,
  font selection, scale policy, and advances with painting. Rasterize missing
  glyph coverage later through existing text finalization.
  `MeasureContext` should expose convenient `measure_text`, `font_metrics`, and
  `line_height` methods, sharing their implementation with `PaintContext`.
  Prepare font access before solver callbacks so these methods need no I/O.
- Store a measurement error outside the solver callback, return a safe finite
  provisional result for that callback, then abort publication after layout.
  Do not turn a failed resource/font measurement into a successful zero-size UI.
- Resolve props, component preparation/update, and match/loop expressions once
  before layout. Repeated measurement must not repeat those operations.

Recommended initial implementations:

- Label: natural unwrapped text advance and line height.
- Button: label metrics plus visual inset and minimum control height.
- Checkbox: checkbox size, text gap, label metrics, and minimum control height.
- Image: prepared pixel size as natural logical size at an explicit default
  one-pixel-per-logical-unit policy, retaining aspect ratio under constraints.
  Image fitting within its allocation remains a painter responsibility.
- Comets/custom canvas: explicit preferred size; animation does not change
  layout merely because its pixel snapshot changes each frame.

Require measurement implementations for painters in this repository rather than
silently retaining a universal row height. Share helpers between measure/paint.
For fixed-size leaves the solver can avoid intrinsic measurement, but the API
must document what happens if those constraints are later removed.

## Frame pipeline and geometry

1. Resolve/prepare the active live tree through the existing walker.
2. Build a prepared hierarchy by traversing template and physical state
   together. Reuse recorded loop item counts, selected match indices, and
   prepared props; do not evaluate expressions again. Map renderable leaves to
   existing physical preorder indices. Preserve container parent/child
   relationships.
3. Build the Taffy tree and compute against the instance viewport. Measure
   leaves through their registered painters, using immutable prepared
   props/state.
4. Derive absolute logical rectangles from parent-relative layout. Distinguish
   border/content boxes and retain ancestor clip intersections for every leaf.
5. Derive content extents, clamp the shared requested scroll for this instance,
   then transform content rectangles consistently for drawing and hit testing.
6. Paint once per leaf into the shared display list, using final dimensions and
   origin. Supply content-box dimensions and origin in `PaintContext`; painters
   do no layout.
7. Finalize text/resources. Publish display output, layout, and input revision
   together only on complete success.

Store an instance-owned geometry snapshot including component bounds in physical
preorder, clips/visible hit regions, container bounds for diagnostics, content
extent, and effective scroll. Temporary Taffy IDs are not input identifiers and
must not escape in `RenderOutput` or actions.

Hit testing uses the same rectangles/transforms/clips as painting, with reverse
paint order for overlapping candidates. Exclude zero-area or fully clipped
regions from pointer targets. Maintain a separate ordered focus/activation list
for all interactive physical components, including offscreen ones. Keyboard
traversal and focused activation use that list, not visible hit regions.
Scrolling focused widgets into view remains deferred.

Retain the most recent pointer position for each instance and clear it on
pointer leave. After successful relayout, recompute hover at the retained
position even without a new motion event. Publish the result to shared
definition hover state and schedule affected peers to redraw. Use the instance
most recently receiving pointer input as the hover source; peer relayout must
not overwrite it. Clearing or removing that source clears hover. This preserves
shared hover without two windows repeatedly overriding one another.

Preserve compatible presented input revisions only when component identities,
activation target mappings, border/content geometry, effective clips, pointer
regions, and focus order remain compatible. Do not compare rectangles alone.
Keep stale-input rejection when any of these change. Conservatively invalidate
on structural changes rather than equating a reused physical index with the
same logical target.
Measurement/layout/paint errors keep the prior published frame and geometry;
component preparation mutations retain the existing nontransactional contract.

## Scrolling, scale, and invalidation

Retain one vertical viewport scroll initially. Use a viewport-sized root and a
natural-height content subtree; do not shrink every control to fit the window
instead of creating scroll extent. Default controls should not flex-shrink below
their minimums. Prototype this root/content arrangement before fixing API
semantics for percentage heights and fill-remaining-space within scroll content.
Taffy calculates boxes, not wheel handling, clipping commands, or scroll
offsets. Clip horizontal overflow initially; horizontal and nested scrolling are
deferred.

Keep all geometry in logical units. Disable default integer-logical-pixel
rounding initially, so fractional display scale does not introduce mismatches.
Use identical final floats for painting and hits. Device-pixel snapping can be
added deliberately later, rounding shared edges consistently at instance scale.

Layout inputs include viewport, relevant presentation/theme metrics, resolved
text/language, image dimensions, prepared state used by measurement, and active
tree structure. Relayout after these change, including resource hot reload.
Hover/focus visuals should not alter intrinsic size. Rebuild and recompute
layout on every normal render in this milestone, including animation frames.
Add an explicit layout timing stage before introducing reuse. Absence of actions
is not evidence that layout is unchanged: preparation, resize, language, scale,
and resource changes can also affect it.

Defer layout dirty tracking, persistent Taffy caching, and fine-grained
dependency propagation to a measured optimization. Any later reuse must prove
that all measurement inputs, tree structure, and presentation constraints are
unchanged.

Do not add virtualization or skip offscreen painter calls in this milestone.
Clipped painters can still request animation or schedule future redraws. A later
culling optimization needs an explicit policy for offscreen animation and must
keep measurement and keyboard targets available.

## Implementation checklist

- [ ] Prototype root scroll sizing plus
  Taffy leaf measurement with the pinned version and explicit box defaults.
- [ ] Add the dependency and engine layout module, style/constraint types,
  adapter, geometry snapshot, and numeric validation.
- [ ] Add Flex containers, basic Grid support, and leaf styles; update state
      reconciliation, walkers, registration, i18n extraction, and exhaustive
      LivePart matches.
- [ ] Add painter measurement dispatch and shared font/visual metrics; implement
  standard and example painters without measurement side effects.
- [ ] Preserve physical hierarchy after preparation and integrate
  prepare → measure/layout → paint → finalize → publish.
- [ ] Replace row-derived bounds/content height with instance geometry, clips,
  scroll extents, separate keyboard targets, hover recomputation, and compatible
  input-revision handling that includes clipping and target mappings.
- [ ] Add layout timing to diagnostics and the F11 overlay.
- [ ] Migrate todo/showcase to nested rows/columns and sensible intrinsic/fixed
      image/canvas sizes; include a Grid form. Retain all
      actions and default-enabled hot reload.
- [ ] Update architecture, UI/painter API docs, and examples. Record adopted
  sizing ownership and container semantics in a decision record.
- [ ] Add the tests below and run `./n check` after each work/fix unit.
- [ ] Record native visual verification before moving the plan to completed.

## Verification

- Numeric layout fixtures: nested rows/columns, padding/margins/gaps, min/max,
  grow/shrink, percentage sizes, aspect ratio, empty containers, and zero
  viewport. Test Grid automatic placement, fixed/auto/fraction
  tracks, min/max tracks, spans, nested Flex/Grid, and loop filtering/repacking.
  Include a long unbroken label, over-constrained padding, trailing margins,
  and fixed-size containers whose clipped descendants must not expand scrolling.
- Measurement: changing painter/font changes intrinsic size; explicit
  constraints win; repeated probes do not repeat prepare/actions or mutate
  state. Measurement and painting agree on text normalization, visual metrics,
  and box models.
- Structure: loop expansion, nested loops, match switches/no match, fragment
  transparency, and inactive registration/i18n extraction. No phantom box/gap.
- Instances: identical definition at different widths/scales/languages produces
  independent geometry while shared focus/hover/requested scrolling remain
  shared.
- Input: nested absolute coordinates, scrolling and ancestor clipping, overlap
  order, boundaries, stale clicks, compatible animation frames, and zero-area
  hits. Verify clipped widgets cannot hover or activate by pointer, offscreen
  keyboard traversal/activation remains available, stationary-pointer hover
  updates after relayout, and peer windows do not fight over shared hover.
  Equal rectangles with changed clips or action mappings must invalidate input.
- Failure: measurement/font/layout/painter errors retain prior output and hit
  geometry; non-finite values fail clearly rather than corrupting input.
- Resources: image replacement with new dimensions relayouts; translated text
  changes natural width; fixed-size animation does not change geometry.
- Performance: record preparation, physical/Taffy tree construction, layout,
  painting, and measurement-call counts for representative 100- and 1,000-item
  loops. Compare static and animated frames; report viewport and backend. Use
  results to prioritize later caching or culling, not brittle timing assertions.
- Render both backends against the same layout fixtures. Keep required native
  behavior checks separate from exact pixel comparisons that can vary by scale.
- Run `./n check`; manually resize both example windows, toggle todo filtering,
  exercise keyboard/mouse/scrolling, and reload longer German text and images.

## Confirmed decisions

- Painters own intrinsic measurement; definitions own constraints and
  containers.
- Use a small engine-owned style API and explicit containers. Composite parts
  remain transparent fragments.
- Defer a unified visual-style/defaults system. Retain the minimum box contract:
  engine padding and painter visual insets have distinct, documented roles.
- Keep text unwrapped and clipped; no wrapping or ellipsis in this milestone.
- Use a viewport root with natural-height vertically scrolling content;
  horizontal overflow clips.
- Images default to natural logical-pixel size. Examples specify sensible sizes.
- Defer baseline alignment and scrolling focused elements into view.
- Make text measurement convenient directly from `MeasureContext`.
- Clip containers by default; use the explicit border/content-box contract.
- Support basic Flex and Grid with constant definition styles.
- Separate visible pointer targets from keyboard focus/activation targets.
- Recompute hover after layout using the active pointer source instance.
- Recompute layout initially; defer reuse, culling, and virtualization.

## Remaining prototype issue

Validate percentage heights and fill-remaining-space inside natural-height
scrolling content with the chosen Taffy version. Document definite versus
indefinite axes before exposing builders whose behavior depends on them. The
viewport root remains definite; the scrolling content's natural height must not
silently become a viewport-height cap.

## Sources

- [Taffy architecture and APIs](https://docs.rs/taffy/0.14.0/taffy/)
- [TaffyTree measurement and layout API](https://docs.rs/taffy/0.14.0/taffy/struct.TaffyTree.html)
- [Taffy leaf-layout helper](https://docs.rs/taffy/0.14.0/taffy/fn.compute_leaf_layout.html)
- [Taffy style: Flex and Grid properties](https://docs.rs/taffy/0.14.0/taffy/style/struct.Style.html)
