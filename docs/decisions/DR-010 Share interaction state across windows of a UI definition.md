# DR-010 Share interaction state across windows of a UI definition

- Status: Accepted
- Date: 2026-10-06

## Decision

Store focus, hover and requested scroll in `UiDefinitionState`. Input from any
instance updates that state and invalidates every instance of the definition.
The engine detects hover for every prepared component and supplies it through
`PaintContext::hovered`, whether the component has an action or not.

This supersedes the independent interaction ownership in
[DR-004](<DR-004 Render UI instances on the worker and present display lists on the GUI thread.md>).
Its worker rendering and GUI presentation architecture remains in use. Component
state, geometry, hit regions, settings and presentation revisions remain per
instance. Each viewport derives its own clamped scroll from the shared request.

## Context

Multiple windows present the same UI with different themes, locales and sizes.
Hovering, focusing or scrolling in one should be reflected in the other views.
Previously hover detection used only activation regions, excluding labels and
other noninteractive components.

## Rationale

Definition ownership models one interactive UI with multiple presentations.
Central state avoids synchronization between copies. Per-instance hit testing
still uses the geometry actually displayed in the originating window. Keeping
scroll clamping out of shared rendering state avoids dependence on viewport size
or the order in which windows are rendered.

## Consequences

- The most recently processed interaction wins across windows. Pointer exit
  clears shared hover; this is one shared selection, not multiple local hovers.
- Hidden instances retain dirty state and apply shared interaction when shown.
- Interaction changes affect only the matching definition. Content invalidation
  clears positional focus and hover across definitions.
- Component identity currently follows prepared order, including noninteractive
  nodes. Presentations must keep corresponding component order. Differing tree
  structures will need stable component keys.
- Focus traversal visits action-bearing components; hover includes all
  components.
- Scroll changes invalidate every peer's geometry. Larger viewports may clamp
  the shared request differently without changing it.

## Considered alternatives

- Keep interaction per instance: allows independent interaction, but fails the
  required synchronized presentations.
- Mirror state through window messages: duplicates state and creates ordering
  concerns despite all interaction already being owned by one worker.
- Share complete physical state and layout: viewport geometry and component
  preparation depend on presentation settings and should remain independent.
- Have painters perform hover hit testing: duplicates engine logic, excludes
  components without custom implementations, and mixes geometry with appearance.
- Add stable component keys immediately: useful for different physical trees,
  but current presentations use corresponding component order; defer that larger
  change until those differing structures require it.
