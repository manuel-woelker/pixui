# Painter styles

Styles belong to the application; painters declare which slots they consume.
Theme tokens supply semantic colors, fonts and control metrics. A family is a
pure function from tokens to a concrete `PaintStyle`, with named variant patches
and interaction color patches. Light and Dark token sets can be replaced for
branding without modifying painters.

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
    style.fill = Some(tokens.accent);
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

Interaction patches contain only colors. They cannot change fonts, insets,
minimum height, border width or border presence. This lets stationary-pointer
hover be recomputed after layout without another measurement pass. The base
recipe and dynamic assignment resolver run once; solver probes reuse the same
bound painter and concrete styles. State color refresh does not rerun recipes.

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

The first version supports opaque fills, borders, text fonts/colors and scalar
insets/minimum heights. There is no inheritance, selector language, cascade,
transition, radius, shadow, gradient or automatic layout-style coupling. Text
input uses one common foreground for selected and unselected text; choose a
selection fill with sufficient contrast. Distinct selected-text foregrounds
would require extra clipped text commands.

See the showcase's **Styles** page for recipes, variants, local overrides,
dynamic assignments and different window fonts.
