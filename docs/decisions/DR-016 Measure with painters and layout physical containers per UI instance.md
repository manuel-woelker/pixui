# DR-016: Measure with painters and layout physical containers per UI instance

- Status: Accepted
- Date: 2026-10-08

## Decision

Use Taffy privately on the application worker for Flex and basic Grid
containers. Definitions declare constant engine-owned styles and explicit
`ContainerPart` boxes. Composite fragments, loops and matches remain
transparent. Painters own pure intrinsic measurement; Taffy assigns final
rectangles, retained separately per `UiInstance`. Containers clip by default.

Prepare once, build a temporary physical tree, measure/layout, paint once, then
publish output and geometry together. Separate keyboard targets from visible
pointer targets. Recompute full layout initially and measure its cost before
adding persistent layout caches or culling.

## Context

Fixed rows cannot express nested toolbars or aligned forms. One definition can
appear at different window sizes, scales and languages. Component semantics are
independent of painter registration, while fonts, visual insets and natural
control sizes depend on the painter. Existing shared interaction state must
continue working with instance-specific geometry.

## Rationale

Painter measurement keeps natural size consistent with drawing without making
components depend on their visual implementation. Explicit containers preserve
fragment and loop composition. A temporary tree follows actual expanded items
and active match arms without evaluating expressions twice. Engine-owned styles
limit public behavior to what painting and input actually support.

Border-box constraints, content-box painting, default clipping and common hit
geometry make padding and overflow predictable. Separate keyboard targets allow
offscreen activation while excluding invisible pointer targets. Logical floats
avoid mismatched rounding between drawing and input at fractional display
scales.

## Consequences

- Every painter implements measurement, which may be called repeatedly without
  mutation, actions, I/O, glyph coverage allocation or animation scheduling.
- Geometry, ancestor clips and scroll clamps are per instance. Focus, hover and
  requested scrolling remain shared by definition; positional identity remains
  a limitation for independently conditional presentations.
- Layout styles are constant initially. Future expression-based styles must
  resolve per physical node, without mutating reused loop templates.
- Containers and components clip; text remains unwrapped. Baselines,
  scroll-into-view, arbitrary overflow and advanced Grid features are deferred.
- Tree construction and solving consume worker time on every normal frame,
  including animation. Diagnostics expose that cost. Offscreen painters still
  run, preserving animation scheduling until a culling policy is defined.
- Published output/geometry survive errors, while already completed preparation
  updates retain the existing nontransactional behavior.

## Considered alternatives

- **Component-owned measurement:** rejected because fonts and visual chrome
  vary by painter. Injecting painter metrics into components adds indirection.
- **Make every composite a box:** rejected because grouping would unexpectedly
  change gaps, nesting and loop item placement.
- **Expose all Taffy styles:** rejected because it couples the API to upstream
  versions and implies support for painting/input behavior we do not implement.
- **Custom Flex/Grid solver:** rejected because implementing constraints and
  intrinsic sizing duplicates an established layout engine.
- **Native/GUI-thread layout:** rejected because worker-side drawing and input
  need the same geometry, and multiple renderers should share one layout.
- **Persistent trees and dependency tracking immediately:** deferred because
  positional reconciliation and arbitrary prepared state complicate
  invalidation. Full layout supplies a correct measurable baseline first.
