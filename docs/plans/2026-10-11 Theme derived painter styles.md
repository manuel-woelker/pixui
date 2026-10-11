# Theme derived painter styles

Status: proposed outline; API names are illustrative.

## Goal

Define reusable style objects independently of component painters, derive them
from theme colors/fonts/dimensions, and assign them to named painter slots.
Expose slots as reflected painter fields of type `StyleIndex`. Resolve styles
into a table owned by `RenderOutput`; painters and draw commands use its
indices. Support semantic variations, interaction states and local overrides.
The same UI definition must continue to render in different themes in separate
windows.

Start with Rust declarations and explicit assignment. A CSS parser, selectors,
implicit inheritance, arbitrary token expressions and style animations are
outside the initial scope.

## Current constraints

- `PresentationSettings::theme` selects Light/Dark per instance. Painters call
  `Palette::for_theme` and embed their own font sizes, insets and border widths.
- `PaintContext` and `MeasureContext` text helpers select Geist internally.
  `FontConfig` already supports another face, logical size and rasterization
  scale; display lists already carry indexed font resources.
- Focus and hover are shared by definition. Checked/active state comes from
  component props/state; disabled and pressed are not universal engine states.
- Text inputs use painter-provided caret geometry. Changing a font or inset must
  change measurement, text placement, selection and hit testing together.
- Layout controls allocated boxes; painters measure intrinsic content and draw
  within their content box. Existing draw commands support opaque colors and
  rectangular fills/strokes. Styling cannot silently introduce rounded corners,
  alpha blending or unsupported border rendering.

## Proposed concepts

| Concept | Responsibility |
| --- | --- |
| Theme tokens | Concrete semantic colors, font roles and dimension roles |
| Style definition | Reusable recipe deriving a style family from those tokens |
| Style family | Base style plus named semantic variants and state patches |
| Painter slot | Named use within one registered painter, e.g. body or focus ring |
| Slot assignment | Selects a style family and optional variant for that slot |
| Style override | Sparse replacement of selected properties or state patches |
| Resolved style | Validated concrete values borrowed during measurement/painting |

### Theme tokens

Use a small typed `ThemeTokens` struct rather than a map of arbitrary names.
Suggested colors include canvas, surface, foreground, muted foreground, accent,
on-accent, border, selection and on-selection. Include regular/mono font roles,
logical font sizes, control insets and stroke widths.

Defaults reproduce Light/Dark. Applications can replace the token sets for those
modes, creating branded themes without replacing painters. Initially keep the
existing Light/Dark selection; named arbitrary theme IDs can follow when
exercised. Tokens contain values, not references to other tokens, avoiding
cycles and recursive evaluation. Resolve recipes only on the worker, without
I/O.

### Style objects and families

A common `PaintStyle` can contain:

- Optional fill and optional border (`color`, logical stroke width).
- A text style (`FontFace`, logical size, foreground).
- Painter content insets and a minimum intrinsic control height.

All base properties have concrete defaults; absence of a fill/border is an
explicit value. A `StylePatch` uses optional replacements. Distinguish “leave
unchanged” from “remove border/fill”, for example `Option<Option<BorderStyle>>.
Merge fields explicitly; do not treat zero as an unset value.

Recipes are ordinary Rust functions over theme tokens. Different families can
share token values while choosing different combinations, such as a primary
button using accent/on-accent and a quiet button using surface/foreground.
Named variants such as `primary`, `danger` and `compact` are family-local;
they do not change component identity, behavior or props.

Keep the common object small. Painter-specific properties such as checkbox
mark size can remain in the painter initially. Add typed extensions only when
an example needs them; avoid a universal property bag of unused values.

### Painter slots and assignment

A painter declares its slots as ordinary reflected `StyleIndex` fields:

```rust
#[pixui_reflect::reflect]
mod button_painter {
    pub struct ButtonPainter {
        pub body: StyleIndex,
        pub focus_ring: StyleIndex,
    }
    // The ordinary Painter<ButtonComponent> trait implementation uses these fields.
}
```

This sketch omits imports and the trait implementation. Field names are the slot
names; no second enumeration is needed. Implement `Reflect` for the concrete,
copyable `StyleIndex` type. At painter registration, inspect its descriptor and
cache the field indices whose exact type is `StyleIndex`. Field order is an
implementation detail; assignment uses the names once, then cached indices.
Defaults are supplied by theme/style assignment code independently of the
painter, with every discovered slot assigned before preparation succeeds.

| Painter | Reflected fields |
| --- | --- |
| Button | body, focus_ring |
| Label | text |
| Checkbox | body, box_style, mark, focus_ring |
| Text input | body, selection, caret, focus_ring |

A slot exposes a style object; a painter uses only its relevant fields. The
body style can supply fill and text. Selection needs fill and selected-text
color if selected foreground rendering is supported. Names are scoped to the
registered painter. Reject missing/unknown assignments and unknown variants.

**Important ownership rule:** the registered painter cannot hold changing
output-local indices while it is shared by several windows and occurrences.
Build a bound painter value for each prepared occurrence. Resolve that
occurrence's theme, variant, state and overrides, insert/reuse its styles in the
output builder, then populate its fields with those `StyleIndex` values. Reuse
this value for measurement, caret geometry and painting of that occurrence;
never rewrite the registered painter to switch between windows.

Existing reflection reads fields and constructs structs but does not mutate
fields. For the initial all-slot painter structs, use the reflected field-order
constructor with generated index values; this needs no general field setter.
Cache its descriptor at registration. Non-slot configuration fields need an
explicit follow-up choice: preserve them through a factory/template mechanism,
or add checked mutable field access. Do not silently reset or discard them.
Stateless unit painters remain valid with an empty schema. No reflected method
invocation is needed in the draw path.

Assignments have two scopes initially:

1. Application painter defaults: remap slots for every use of that painter.
2. `ComponentPart` overrides: choose another family/variant or patch a slot for
   that declaration, applying to its physical occurrences.

A narrow style resolver on `ComponentPart` can derive assignments/patches from
its expression context and presentation settings, just like existing props and
focus resolvers. Resolve it once during preparation, not during each draw call
or layout probe. This permits a loop row to choose `danger` based on data.

No subtree cascade initially. An application can reuse an ordinary Rust bundle
of assignments for several declarations. Add definition/subtree scopes later
only if explicit reuse becomes cumbersome.

## Variations and override precedence

Separate semantic variants from transient interaction states. A `primary`
button can simultaneously be hovered and focused; those are not competing
exclusive variants.

Recommended resolution order, property by property:

1. Theme-derived family base.
2. Assigned semantic variant patch.
3. Applicable state patches in explicit order: active/checked, hover, disabled.
4. Declaration-local override, including any local state-specific patches.

A separate focus-ring slot is drawn over the body, so focus remains visible
without fighting its fill/text colors. State priority is documented per family;
combined states can be represented by explicit predicates/patches when needed.
Do not use registration/hash-map iteration order to choose a winner.

Local explicit values win, including a forced fill during hover. To customize
only hover, override the hover patch rather than the base fill. This distinction
must be shown in the example/API documentation.

Focus/hover come from the engine; a painter derives supported semantic states
from typed props/state. A visual disabled style does **not** disable activation,
focus or editing. Behavior must be controlled separately. Do not invent a
pressed state until input tracks it correctly.

Initially permit interaction patches to change colors only. Fonts, insets and
stroke widths stay stable across hover/focus to avoid layout jumps and feedback
loops when hit boxes move under the pointer. Semantic variants and data-driven
local overrides may change metrics and therefore require layout preparation.

## Resolution, ownership and invalidation

The application worker owns the style registry and immutable theme snapshots.
Preparation resolves the current instance's theme, assignments and overrides
into a small slot bundle. Borrow the same bundle in `MeasureContext`,
`Painter::text_input_geometry` and `PaintContext`. Repeated measurement probes
must not resolve recipes, allocate atlases, mutate styles or perform I/O.

`StyleIndex` is local to one output, not a persistent family ID or field index.
Maintain the same slot ordering in the worker's prepared style table and the
immutable `RenderOutput::styles` table. The former contains font configurations
for measurement/glyph collection; finalization freezes renderer-facing entries
with font indices into the existing font resource table. Retained outputs own
all resources needed to render their styles, independently of later updates.

Add indexed drawing helpers such as `context.fill_rect(rect, self.body)` and
`context.text(origin, text, self.body)`. Commands reference `StyleIndex` rather
than repeating style colors/widths/fonts; geometry and the command's role select
fill, stroke or text properties. Keep raw-value commands/helpers available for
custom image/animation drawing. Styles with an absent fill/border produce no
corresponding command. Validate indices and required command properties before
publishing an output.

Expose font-aware text measurement/drawing helpers using a resolved text style.
Both paths construct the same `FontConfig` with the instance's scale. Keep
existing size-only helpers as documented Geist conveniences for custom painters
while migrating standard painters to styles. Custom fonts must not require a
host text renderer or a second glyph cache.

Painter insets describe content drawn/measured within the painter's content
box. `LayoutStyle` still controls outer dimensions, margin, layout padding and
layout borders; avoid applying an inset twice. A painted stroke initially stays
inside that content box and does not independently alter Taffy's box model.
Document this distinction rather than silently linking two border widths.

Use a checked revision on registry/theme updates. A theme or family replacement
validates fully before publication; failures retain the previous snapshot.
Invalidate affected UIs (all instances is an acceptable first implementation),
including measurement and input geometry when metrics can change. Hidden
instances defer preparation until shown; retained outputs keep old resources.
Style edits preserve stable identity, shared focus and selection.

Cache theme-derived families by theme/registry revision if useful; start with
ordinary vectors/maps. Local resolved bundles need not be globally cached.
Never key caches solely by Light/Dark or retain arbitrary closures in outputs.
GUI clients receive concrete style snapshots, indexed draw commands and
resources, without reflection or theme resolution. Today renderers consume
`DisplayList` directly. Keeping styles on `RenderOutput` requires a borrowed
render input containing both the list and its styles; update both renderer
implementations, CPU drawing, validation, diagnostic output reuse and tests
accordingly. Never let a display list with style indices lose its owning table.
Deduplicate entries using resolved values and font identity, not slot names;
start with straightforward table building and optimize only if measured.
Resolve canvas/background through theme tokens too, avoiding a styled UI painted
on an unrelated hard-coded palette background.

## Illustrative flow

```rust
// Conceptual assignment API; the painter declares body/focus_ring through reflection.
let primary = styles.define("primary control", |theme| {
    StyleFamily::new(PaintStyle {
        fill: Some(theme.accent),
        text: theme.body_text.with_color(theme.on_accent),
        ..theme.control_style()
    }).hover(StylePatch::fill(theme.accent_hover))
});
styles.assign::<ButtonPainter>("body", primary)?;

// Inside the bound painter: no name lookup or theme evaluation.
context.fill_rect(context.bounds(), self.body);
context.text(origin, label, self.body)?;
```

The sketch demonstrates independent recipes and reflected assignment. Exact
builder signatures remain to be finalized. The token struct must include every
role used by shipped recipes, including `accent_hover` here.

## Implementation checklist

- [ ] Finalize reflected slot binding/construction, common style fields and
      state precedence; explicitly decide support for non-slot painter fields.
- [ ] Add typed theme tokens, concrete styles, sparse patches and validated
      family registration; provide Light/Dark defaults matching existing output.
- [ ] Discover reflected `StyleIndex` fields, validate external default
      assignments and cache field/name lookup at registration.
- [ ] Add declaration-local assignments/overrides and a pure dynamic resolver.
- [ ] Build occurrence-specific bound painters and prepared style tables; share
      them across measurement, painting and text-input caret geometry.
- [ ] Freeze `RenderOutput::styles`, add indexed drawing helpers/commands and
      update renderer inputs, validation and diagnostics to retain the table.
- [ ] Add font-style measurement/drawing helpers and migrate standard painters
      and canvas background from hard-coded palette/constants.
- [ ] Add validated runtime theme/style replacement and invalidation.
- [ ] Extend the showcase with primary/quiet/danger styles, shared token reuse,
      a local color override and different themes/fonts across its two windows.
- [ ] Update architecture, painter/text/input documentation and record the
      significant ownership/precedence decisions in a decision record.
- [ ] Run `./n check` after each unit; record manual verification before moving
      the plan to completed.

## Verification checklist

- [ ] Token derivation, sparse merge/removal and deterministic
      variant/state/local precedence, including hover+focus+checked
      combinations.
- [ ] Reflected slot discovery and construction, missing/duplicate/foreign
      assignments, unbound/out-of-range output indices, unknown variants,
      nonfinite/negative metrics and invalid font configurations fail clearly.
- [ ] Runtime replacement is atomic; errors keep last-good styles and outputs.
- [ ] Multiple occurrences bind distinct overrides without mutating the shared
      registered painter; old outputs retain independent style/resource tables.
- [ ] One definition renders different themes/fonts in two instances without
      styles, focus or selection leaking across definitions/applications.
- [ ] Font/inset changes agree across intrinsic sizing, glyph placement, text
      selection, caret/IME rectangles and pointer hit testing at different DPI.
- [ ] Hover/focus colors preserve layout and safe input revision compatibility;
      metric/theme changes reject stale geometry and retain logical identity.
- [ ] Repeated solver probes reuse prepared styles; color-only changes reuse
      existing font/image resource identities without unnecessary rasterization.
- [ ] Hidden-window updates, keyed loops and conditional removal retain the
      existing state/resource lifecycle contracts.
- [ ] Software/femtovg agree for styled rectangles, text and clipping. Manually
      inspect contrast, focus visibility, selection and resized example windows.

## Open questions and recommended defaults

- **Slot API:** reflected `StyleIndex` fields are the selected direction.
  Keep persistent family IDs distinct from output-local indices. Decide how
  configurable painters preserve non-slot fields before imposing `Reflect` on
  existing custom painters; an optional reflected binding path can preserve
  compatibility for painters using raw-value commands.
- **Style states:** provide a pure painter hook to derive
  checked/active/disabled flags from typed props/state, combined with engine
  hover/focus, before binding the style fields. Reflection alone cannot infer
  those semantics.
- **Variants:** start with one named variant per family; combinations such as
  danger+compact can use explicit recipes. Avoid a combinatorial variant system.
- **Dimensions:** include font size, painter inset and minimum control height;
  keep outer layout in `LayoutStyle`. Do not add automatic style/layout
  coupling.
- **Selected-text foreground:** current input emits one text command. Supporting
  a distinct selection foreground requires clipped additional text drawing;
  choose whether to include it now or retain a contrast-safe common foreground.
- **Theme extensibility:** replacing Light/Dark token sets is the initial scope.
  Decide whether a concrete application already needs multiple named themes.
- **Additional drawing properties:** radius, opacity, gradients and shadows need
  explicit renderer capabilities and validation, not just extra style fields.

The main risks are mismatched measurement/painting fonts, ambiguous precedence,
layout-changing interaction states and expanding this into a CSS engine. Keep
those contracts explicit and verify them before adding inheritance or caching.
