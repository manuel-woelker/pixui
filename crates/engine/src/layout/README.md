# Component layout

Layout runs on the application worker using a private Taffy adapter. Definitions
contain constant logical-pixel styles; each `UiInstance` retains its own final
geometry for its viewport, language, and scale. GUI backends only draw commands.

## Composition

`ContainerPart::row()` and `column()` select Flex layout. `grid()` selects Grid;
`with_columns` and `with_rows` configure Grid tracks. Fragments
(`CompositePart`), loops, and matches add no box: their active physical children
become items in the enclosing container. Wrap a repeated card in a container if
it should occupy one Grid cell. A filtered automatic Grid repacks remaining
items.

```rust
use pixui_engine::{
    layout::{container::ContainerPart, grid::Track, style::LayoutStyle},
    live_model::part::{ComponentPart, LivePart},
};

// Default components are inert walker nodes; a real UI uses registered typed
// components. The composition and styles are the same.
let card: LivePart = ContainerPart::column()
    .with_gap(8.0)
    .with_padding(12.0)
    .with_children(vec![
        ComponentPart::default().with_layout(LayoutStyle::fixed(80.0, 24.0)).into(),
    ])
    .into();
let grid = ContainerPart::grid()
    .with_columns(vec![Track::auto(), Track::fraction(1.0)])
    .with_gap(8.0)
    .with_children(vec![card]);
```

Common box and parent-item properties belong to `LayoutStyle`. Container
algorithm settings belong to `ContainerLayout::{Flex, Grid}`. For example, a
Grid container can grow as an item in a Flex row. Styles are validated at UI
registration and rendering; nonfinite/negative lengths and invalid placements
are errors. Percent values use fractions (`0.5` means 50%). Margins are
nonnegative logical lengths in this version. `Auto` min sizes retain intrinsic
minimums; use zero minimums for deliberately shrinkable content.

Flex defaults to no growth, shrink enabled, auto basis, stretch alignment, and
no wrapping. Direction and distribution can be set through `FlexLayout`. Flex
wrapping moves whole widgets; it does not wrap text. Grid defaults to row-major
automatic placement, stretch alignment, and auto implicit tracks. Explicit track
starts are positive one-based indices; spans are positive. Fraction tracks use
a zero minimum and share remaining space; `Track` permits explicit min/max
bounds. Named areas, negative lines, dense placement and repeat syntax are not
exposed. Paint and keyboard order always follow physical definition order.

## Measurement and boxes

`Painter::measure` receives read-only prepared props/state, presentation
settings, known content dimensions and available space (definite, min-content,
or max-content). It may run repeatedly. Respect known dimensions and return
finite nonnegative content size. Do not prepare resources, mutate state,
dispatch, schedule animation, emit commands, or allocate glyph coverage.
`measure_text`, `font_metrics`, and `line_height` share painting's metrics and
normalization, without a display-list sink. Text does not wrap or ellipsize.

- Explicit sizes and min/max constraints describe border boxes. Padding is
  included. There is no engine border style yet.
- Painter natural sizes include visual chrome, such as button text insets, but
  exclude engine-managed padding. `PaintContext` dimensions and origin are the
  content box after padding. Taffy may enlarge tiny boxes to contain padding;
  content dimensions are clamped to zero.
- Containers clip descendants to their border boxes by default. Component
  drawing clips to the content box; the viewport always clips. Interactive
  targets use the border box (including padding, excluding margin), intersected
  with ancestor clips and the viewport. Hover applies to all components using
  these same clipped border boxes.
- Keyboard targets retain all interactive physical components, including fully
  clipped ones. Focus traversal and activation do not depend on pointer
  visibility.

Preparation, expressions, and state updates run once before solving. A temporary
physical hierarchy uses the recorded loop counts and selected match arms.
Painters then measure, Taffy allocates, and painters draw once into the shared
display list. No layout IDs leave the worker. Failure preserves published
output/geometry; preparation mutations are not rolled back.

## Scroll and revisions

An implicit root column supplies 16 logical pixels of outer padding and 8 pixels
of gap. Width is definite; scrolling height is natural, with at least the
viewport height. Percentage heights resolve as auto when the enclosing height is
indefinite; an explicitly sized ancestor provides a definite height. Min-height
alone does not provide a definite percentage-height reference. Vertical extent
includes trailing margins. Fixed containers clip oversized descendants without
expanding the outer scroll extent; horizontal overflow clips. Fill-height
content must have an explicit definite ancestor, rather than treating scrolling
content as viewport-height constrained.

Scroll requests remain shared by definition; each instance clamps independently.
Geometry uses unrounded logical floats for both painting and input. Instance
snapshots contain border/content boxes, effective clips, container bounds,
pointer regions and separate keyboard targets. Compatible visual redraws keep
published action bindings only while geometry, clips, component registrations,
activation factories, and target ordering match. Content invalidation and
presentation changes install fresh targets and reject old input revisions.

The most recently active pointer instance supplies shared hover. Its retained
pointer position is tested against new geometry before painting; peers redraw
with that shared result. Hiding, closing or leaving the active source clears
hover. Leaving an inactive peer does not clear another window's hover.

Full layout runs on every normal render initially. F11 reports tree construction
and solving separately; worker timings also record measurement-call count.
Layout reuse, virtualization and offscreen painter culling are deferred.
Offscreen painters still run and can request animation; clipping alone does not
suspend component work.
