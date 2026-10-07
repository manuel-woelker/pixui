//! Native icon conversion happens only for a changed worker-side snapshot.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::ui::image::{Image, ImagePixels, RgbaColor};
use winit::window::Icon;

pub(crate) fn from_image(image: &Image) -> PixuiResult<Icon> {
    Icon::from_rgba(rgba(image), image.width(), image.height())
        .map_err(|error| pixui_error!("create native window icon: {error}"))
}
fn rgba(image: &Image) -> Vec<u8> {
    match image.pixels() {
        ImagePixels::Rgb {
            pixels,
            transparent_color,
        } => pixels
            .iter()
            .flat_map(|color| {
                [
                    color.0,
                    color.1,
                    color.2,
                    if Some(*color) == *transparent_color {
                        0
                    } else {
                        255
                    },
                ]
            })
            .collect(),
        ImagePixels::Rgba { pixels } => pixels
            .iter()
            .flat_map(|&RgbaColor(r, g, b, a)| [r, g, b, a])
            .collect(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use pixui_engine::ui::display_list::Color;
    #[test]
    fn icon_pixels_preserve_rgba_and_rgb_color_keys() {
        let image =
            Image::new_rgba(2, 1, vec![RgbaColor(1, 2, 3, 0), RgbaColor(4, 5, 6, 128)]).unwrap();
        assert_eq!(rgba(&image), [1, 2, 3, 0, 4, 5, 6, 128]);
        let image = Image::new(
            2,
            1,
            vec![Color(1, 2, 3), Color(4, 5, 6)],
            Some(Color(1, 2, 3)),
        )
        .unwrap();
        assert_eq!(rgba(&image), [1, 2, 3, 0, 4, 5, 6, 255]);
    }
}
