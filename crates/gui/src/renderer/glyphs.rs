//! The same glyph placement for software sampling and GPU quads.
use pixui_engine::ui::{
    geometry::{Point, Rect},
    text::resource::{FontResource, PixelRect},
};

pub(crate) fn positioned<'a>(
    font: &'a FontResource,
    origin: Point,
    text: &'a str,
    scale: f32,
) -> impl Iterator<Item = (Rect, PixelRect)> + 'a {
    let mut pen = origin;
    text.chars().filter_map(move |character| {
        if character == '\n' {
            pen.x = origin.x;
            pen.y += font.metrics().line_height;
            return None;
        }
        let glyph = font.glyph(character).expect("validated text resource");
        let result = glyph.atlas_rect.map(|source| {
            (
                Rect {
                    x: ((pen.x + glyph.offset.x) * scale).round() / scale,
                    y: ((pen.y + glyph.offset.y) * scale).round() / scale,
                    width: source.width as f32 / font.scale(),
                    height: source.height as f32 / font.scale(),
                },
                source,
            )
        });
        pen.x += glyph.advance;
        result
    })
}
