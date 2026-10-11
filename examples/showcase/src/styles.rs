//! Recipes shared by standard reflected slots, without changing painter code.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    styles::{
        family::{StyleAssignment, StyleFamily},
        style::{Border, PaintStyle, StylePatch},
        theme::{ThemeFont, ThemeTokens},
    },
    ui::{display_list::Color, presentation::Theme},
};
fn primary(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = PaintStyle::text(tokens);
    style.fill = Some(tokens.accent);
    style.foreground = tokens.on_accent;
    style
}
fn quiet(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = PaintStyle::text(tokens);
    style.foreground = tokens.accent;
    style.border = Some(Border {
        color: tokens.accent,
        width: 1.0,
    });
    style
}
fn danger(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = primary(tokens);
    style.fill = Some(tokens.danger);
    style
}
pub fn register(application: &ApplicationHandle) -> PixuiResult<()> {
    for (name, recipe) in [
        (
            "showcase.primary",
            primary as pixui_engine::styles::family::StyleRecipe,
        ),
        ("showcase.quiet", quiet),
        ("showcase.danger", danger),
    ] {
        let mut family = StyleFamily::new(recipe);
        family.variants.insert(
            "compact".into(),
            StylePatch {
                font_size: Some(13.0),
                inset: Some(5.0),
                minimum_height: Some(28.0),
                ..Default::default()
            },
        );
        family.variants.insert(
            "mono".into(),
            StylePatch {
                font: Some(ThemeFont::Mono),
                ..Default::default()
            },
        );
        application.register_style(name, family)?;
    }
    let mut dark = ThemeTokens::for_theme(Theme::Dark);
    dark.font = ThemeFont::Mono;
    application.set_theme_tokens(Theme::Dark, dark)
}
pub fn assignment(application: &ApplicationHandle, name: &str) -> PixuiResult<StyleAssignment> {
    let name = name.to_owned();
    application.inspect(move |app| Ok(StyleAssignment::new(app.styles().family(&name)?)))
}
/// A declaration-local patch demonstrates that its neighboring occurrences
/// continue using the registered family unchanged.
pub fn local_color() -> StylePatch {
    StylePatch {
        fill: Some(Some(Color(100, 45, 150))),
        ..Default::default()
    }
}
