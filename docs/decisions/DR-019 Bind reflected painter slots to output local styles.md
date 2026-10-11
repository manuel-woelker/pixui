# DR-019: Bind reflected painter slots to output local styles

- Status: Accepted
- Date: 2026-10-11

## Decision

Styled painters declare reflected `StyleIndex` fields. External assignments
provide family recipes, one named variant and local patches. Construct a painter
per physical occurrence using reflected field-order construction. Resolve fonts
and metrics before layout and share them with caret geometry and drawing.

Publish concrete, deduplicated styles in `RenderOutput::styles`. Indexed drawing
commands refer to that table; renderer input borrows both tables together.
Retain the original raw custom-painter registration and drawing path.

Use base/variant, family active/hover/disabled, local ordinary override, then
local active/hover/disabled precedence. Focus rings use separate slots.
Interaction patches are color-only. Theme and family replacements validate
candidates before publication and conservatively invalidate all instances.

## Rationale

Reflection exposes slot names without another field list or setters. Independent
recipes let applications change appearance without modifying painter behavior.
Occurrence-specific construction avoids shared painter mutation across windows,
loop items and local overrides. Identical measurement and drawing fonts prevent
layout, hit-test and caret drift.

Output indices keep repeated paint data out of draw commands. Immutable tables
make old outputs independently renderable. Color-only state patches allow hover
to refresh after layout without causing layout feedback loops.

## Context

One definition can be shown in multiple windows with different themes, DPI and
language. Focus and hover are shared by definition; geometry is per instance.
Custom painters may be Send without Sync and hold arbitrary configuration.
Reflection already supports lazily cached descriptors and sendable construction.

## Consequences

Styled painters initially contain slots only. Raw painters remain configurable
and do not require reflection. Defaults must cover every slot, and duplicate or
unknown assignments fail clearly. Replacements must preserve existing variant
names; dynamic resolver outputs are checked during rendering.

Prepared indices may be remapped when final styles are deduplicated; never
retain them. Renderers must receive the full output through `RenderInput`.
Selection currently uses a common text foreground rather than additional clipped
drawing. There is no selector language, inheritance or coupling to outer layout
rules.

## Considered alternatives

- **Mutate the registered painter:** rejected because windows and occurrences
  need independent bindings, and temporary mutation introduces restoration
  rules.
- **Style structs embedded in every command:** rejected because repeated colors,
  border data and fonts increase draw-list size.
- **A second slot macro or handwritten setters:** rejected because existing
  reflection discovers fields and constructs the slot-only object already.
- **Require every painter to be reflected or Sync:** rejected because raw custom
  drawing and worker-owned configurable painters remain useful and simple.
- **Metric changes on hover/focus:** rejected because hit boxes would change
  while resolving those same interaction states.
- **CSS selectors and inherited themes:** deferred because explicit assignments
  and typed semantic tokens meet the current needs with predictable precedence.
