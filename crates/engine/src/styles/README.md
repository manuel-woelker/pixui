# Painter styles

Styles belong to the application; painters declare which slots they consume.
Theme tokens supply semantic colors, fonts and control metrics. A family is a
pure function from tokens to a concrete `PaintStyle`, with named variant patches
and interaction brush/color patches. Light and Dark token sets can be replaced
for branding without modifying painters.

## Reflected slots

Put a painter in a `#[pixui_reflect::reflect(send)]` inline module. Its fields
must all be `StyleIndex`. Reflection discovers names, checks exact field types,
and generates field-ordered construction; no setters or additional field list
are required. Register external defaults with
`Application::register_styled_painter::<Component, Painter>(assignments)`.
`PainterRegistry::slots::<Component>` exposes the discovered schema.

```rust
use pixui_base::PixuiResult;
use pixui_engine::{
    application::app::Application,
    components::button::{ButtonComponent, ButtonProps},
    live_model::part::ComponentPart,
    styles::{family::{StyleAssignment, StyleFamily},
        slots::SlotAssignment, style::PaintStyle},
};
#[pixui_reflect::reflect(send)]
mod painters {
    use pixui_base::PixuiResult;
    use pixui_engine::{components::button::ButtonComponent,
        painters::{painter::Painter, context::PaintContext, measure::MeasureContext},
        styles::index::StyleIndex, ui::geometry::{Point, Size}};
    pub struct Emphasis { pub body: StyleIndex }
    impl Painter<ButtonComponent> for Emphasis {
        fn measure(&self, context: &MeasureContext<'_, ButtonComponent>) -> PixuiResult<Size> {
            let style = context.style(self.body)?;
            let text = style.font_config(context.settings.scale_factor)?.measure_text(&context.props.label)?;
            Ok(context.constrain(Size {
                width: text.width + 2.0 * style.inset,
                height: text.height.max(style.minimum_height),
            }))
        }
        fn paint(&self, context: &mut PaintContext<'_, ButtonComponent>) -> PixuiResult<()> {
            context.styled_fill_rect(context.bounds(), self.body)?;
            context.styled_text(Point {
                x: context.style(self.body)?.inset,
                y: context.styled_baseline(&context.props.label, self.body)?,
            }, &context.props.label, self.body)
        }
    }
}
# fn main() -> PixuiResult<()> {
let mut app = Application::default();
let button = app.register_component::<ButtonComponent>("button")?;
let family = app.register_style("emphasis", StyleFamily::new(|tokens| {
    let mut style = PaintStyle::text(tokens);
    style.fill = Some(tokens.accent.into());
    style.foreground = tokens.on_accent;
    style
}))?;
app.register_styled_painter::<ButtonComponent, painters::Emphasis>(vec![
    SlotAssignment::new("body", StyleAssignment::new(family)),
])?;
let part = ComponentPart::typed(button, |_, _| Ok(ButtonProps { label: "Save".into() }));
# let _ = part;
# Ok(())
# }
```

Raw custom painters retain `register_painter` and raw drawing helpers. They may
hold arbitrary configuration and need neither reflection nor `Sync`. Standard
painters override the registration hook to install their reflected defaults;
construct them with `Default`. A styled painter cannot mix configuration fields
with slots. Such a painter can currently use the raw path instead.

## Assignments and precedence

`ComponentPart::with_style(slot, assignment)` replaces the registered assignment
for that occurrence. `StyleAssignment::variant` selects one named variant;
`patch` supplies sparse local overrides. `StylePatch::fill = Some(None)` removes
a fill; `None` leaves it unchanged. Border removal follows the same rule.
`with_style_resolver` returns assignments from application expressions once per
physical node during preparation. Duplicate or unknown slots are errors,
including collisions between constant and dynamic assignments.

Resolution order is:

1. Family recipe from this instance's theme tokens.
2. One named variant.
3. Family active, hover, disabled patches, in that order.
4. Local ordinary patch.
5. Local active, hover, disabled patches, in that order.

`Painter::style_states` derives semantic active/checked/disabled state from
props/state; the engine adds shared hover after layout. Focus uses a separate
slot, drawn by the painter when focused. Disabled styling does not disable
activation or keyboard focus. No pressed state is inferred.

Interaction patches contain only fills and colors. They cannot change fonts,
insets, minimum height, border width or border presence. This lets
stationary-pointer hover be recomputed after layout without another measurement
pass. The base recipe and dynamic assignment resolver run once; solver probes
reuse the same bound painter and concrete styles. State fill/color refresh does
not rerun recipes.

Font size, inset and minimum control height are painter metrics. Outer padding,
margins, flex/grid rules and fixed sizes remain in `LayoutStyle`. A fixed layout
size takes precedence over intrinsic measurement and may clip the result.

## Output ownership

A fresh bound painter is constructed per physical occurrence and render. The
shared registration is immutable; frame indices never persist in its fields.
Measurement, text-input caret geometry and painting all read that occurrence's
styles. Fonts selected for text use the existing worker glyph batching/cache.

`RenderOutput::styles` holds concrete `RenderStyle` values. Indexed commands
contain `StyleIndex`; text contains only baseline, string and style index. The
worker finalizes glyph demands, fills font indices, deduplicates equal renderer
styles and remaps commands before publishing. Frame indices can consequently
differ from the indices used inside the temporary painter. Do not retain either.

Pass the whole output through `RenderInput::from(&output)` to a backend.
Headless `renderer::render` returns a `RenderedDisplay` containing both tables.
Passing only its `DisplayList` loses styles and correctly fails validation. Raw
lists still convert with an empty style table.
`DisplayListBuilder::finish_styled` returns both tables; its older finish
methods reject styled lists.

Software and femtovg share borrowed command resolution, including validation;
strings and snapshots are not copied while resolving styles. Retained outputs
keep their own styles, images and fonts after theme changes. Memory diagnostics
include the frozen style table.

## Replacement and limits

`set_theme_tokens` and `replace_style` validate candidates against families,
registered defaults and constant declaration overrides before swapping them.
Replacement families must preserve existing variant names so retained
assignments remain meaningful. Failure leaves the registry and existing outputs
unchanged. Dynamic resolver results are checked when rendering; their arbitrary
future values cannot be validated at registration. Rendering errors preserve
the last good output.

Successful replacements invalidate all instances conservatively, including
geometry revisions. Hidden instances defer painting until shown. Logical focus,
selection and keyed component state remain shared and survive valid restyling.
Hover-only color changes retain the existing geometry compatibility policy.

The styles support opaque solid/linear-gradient fills, borders, text
fonts/colors and scalar insets/minimum heights. There is no inheritance,
selector language, cascade, transition, radius, shadow or automatic layout-style
coupling. Text input uses one common foreground for selected and unselected
text; choose a selection fill with sufficient contrast. Distinct selected-text
foregrounds would require extra clipped text commands.

See the showcase's **Styles** page for recipes, variants, local overrides,
dynamic assignments and different window fonts.

## Background brushes

`PaintStyle::fill`, ordinary patches and interaction patches use `Brush`.
Use `color.into()` for a solid fill, or a two-color `LinearGradient`:

```rust
use pixui_engine::{styles::{brush::LinearGradient, style::PaintStyle},
    styles::theme::ThemeTokens, ui::presentation::Theme};
let tokens = ThemeTokens::for_theme(Theme::Light);
let mut style = PaintStyle::text(&tokens);
style.fill = Some(LinearGradient::horizontal(tokens.accent, tokens.danger).into());
```

`vertical` runs top to bottom; custom `start`/`end` points support diagonals and
reversed directions. Coordinates are relative to the original filled rectangle
from (0, 0) to (1, 1), independent of clipping and display scale. Endpoints must
be finite, within that range and distinct; invalid styles are rejected before
publication. Encoded sRGB channels interpolate between two opaque colors and
clamp beyond the endpoints. Alpha, radial gradients and multiple stops are not
yet supported. Borders and text continue to use solid colors.

Gradient brushes live in the deduplicated output style table. Commands still
carry only a style index. Both software and GPU backends support the same fills;
the GPU uses native gradient paints without generating images or textures.
Hover/active/disabled patches can replace the entire brush without relayout.

## Rounded corners

Set `PaintStyle::corner_radius` or `StylePatch::corner_radius` to a nonnegative,
finite logical-pixel radius. The default is zero (square corners); the renderer
clamps larger radii to half the shorter rectangle dimension, allowing pill
shapes without knowing the final size. For example, a variant can use
`StylePatch { corner_radius: Some(12.0), ..Default::default() }`.

Styled fills (including gradients) and borders share this radius. Borders stay
inside the original rectangle; their inner radius is the clamped outer radius
minus the border width, floored at zero. A border thicker than half the shorter
side fills the rounded shape. Rounded edges are antialiased by both backends;
the software fallback uses pixel coverage and the GPU uses native paths.

Radius is paint geometry only. It does not change layout, intrinsic size,
rectangular hitboxes, focus navigation or child/image clipping. Painters must
explicitly assign the desired radius to each slot (including separate focus
slots). Interaction patches still change only brushes/colors. Raw rectangle
commands retain their square behavior.
