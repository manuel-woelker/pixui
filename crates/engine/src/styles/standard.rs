//! Shared semantic recipes for the standard painter slots.
use super::{
    family::{StyleAssignment, StyleFamily},
    registry::StyleRegistry,
    slots::SlotAssignment,
    style::{Border, PaintStyle},
    theme::ThemeTokens,
};
use pixui_base::PixuiResult;
fn body(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = PaintStyle::text(tokens);
    style.fill = Some(tokens.surface.into());
    style
}
fn checkbox(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = body(tokens);
    style.minimum_height = (tokens.minimum_height - 4.0).max(0.0);
    style
}
fn text(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = PaintStyle::text(tokens);
    style.inset = 0.0;
    style.minimum_height = 0.0;
    style
}
fn focus(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = PaintStyle::text(tokens);
    style.border = Some(Border {
        color: tokens.accent,
        width: tokens.stroke_width,
    });
    style
}
fn input_focus(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = focus(tokens);
    style.border.as_mut().expect("focus has a border").width = tokens.stroke_width / 2.0;
    style
}
fn box_style(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = PaintStyle::text(tokens);
    style.border = Some(Border {
        color: tokens.border,
        width: tokens.stroke_width,
    });
    style
}
fn mark(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = PaintStyle::text(tokens);
    style.fill = Some(tokens.accent.into());
    style
}
fn caret(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = PaintStyle::text(tokens);
    style.fill = Some(tokens.foreground.into());
    style
}
fn selection(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = PaintStyle::text(tokens);
    style.fill = Some(tokens.selection.into());
    style
}
fn input(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = body(tokens);
    style.border = Some(Border {
        color: tokens.border,
        width: tokens.stroke_width / 2.0,
    });
    style
}
/// Defaults are external to painters; changing a theme affects every recipe.
pub fn assignments(
    registry: &mut StyleRegistry,
    slots: &[(&str, &str)],
) -> PixuiResult<Vec<SlotAssignment>> {
    for (name, recipe) in [
        ("control", body as super::family::StyleRecipe),
        ("checkbox", checkbox),
        ("text", text),
        ("focus", focus),
        ("input_focus", input_focus),
        ("box", box_style),
        ("mark", mark),
        ("caret", caret),
        ("selection", selection),
        ("input", input),
    ] {
        if registry.family(name).is_err() {
            let mut family = StyleFamily::new(recipe);
            if name == "control" {
                family.active_tokens = Some(|tokens| super::style::StatePatch {
                    fill: Some(tokens.accent.into()),
                    ..Default::default()
                });
            }
            registry.register(name, family)?;
        }
    }
    slots
        .iter()
        .map(|(slot, family)| {
            Ok(SlotAssignment::new(
                *slot,
                StyleAssignment::new(registry.family(family)?),
            ))
        })
        .collect()
}
