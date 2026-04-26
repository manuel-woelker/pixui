use crate::draw::brush::Brush;
use crate::draw::command::DrawCommand;
use crate::draw::draw_list::DrawList;
use crate::draw::draw_style::DrawStyle;
use pixui_base::err;
use pixui_base::result::{OptionExt, PixuiResult};
use pixui_base::shared_string::SharedString;
use std::fmt::Write as _;

/// Renders a draw list into an SVG document.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SvgRenderer;

impl SvgRenderer {
    /// Renders the draw list into SVG file contents.
    pub fn render(draw_list: &DrawList) -> PixuiResult<SharedString> {
        let mut active_style = None;
        let mut body = String::new();

        for command in &draw_list.commands {
            match command {
                DrawCommand::SelectStyle { style_id } => {
                    let style = draw_list.styles.get(style_id.index()).with_context(|| {
                        format!(
                            "Draw command selected unknown style id {}",
                            style_id.index()
                        )
                    })?;
                    active_style = Some(style);
                }
                DrawCommand::FillRoundedRectangle {
                    x,
                    y,
                    width,
                    height,
                    radius,
                } => {
                    let style = active_style.context(
                        "Draw command requires an active style before drawing a filled rounded rectangle",
                    )?;
                    Self::write_filled_rounded_rectangle(
                        &mut body, style, *x, *y, *width, *height, *radius,
                    )?;
                }
                DrawCommand::OutlineRoundedRectangle {
                    x,
                    y,
                    width,
                    height,
                    radius,
                } => {
                    let style = active_style.context(
                        "Draw command requires an active style before drawing an outlined rounded rectangle",
                    )?;
                    Self::write_outlined_rounded_rectangle(
                        &mut body, style, *x, *y, *width, *height, *radius,
                    )?;
                }
                DrawCommand::DrawText { x, y, text } => {
                    let style = active_style
                        .context("Draw command requires an active style before drawing text")?;
                    Self::write_text(&mut body, style, *x, *y, text.as_str())?;
                }
            }
        }

        Ok(SharedString::from(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\">{}</svg>",
            Self::format_number(draw_list.bounds.x),
            Self::format_number(draw_list.bounds.y),
            Self::format_number(draw_list.bounds.width),
            Self::format_number(draw_list.bounds.height),
            Self::format_number(draw_list.bounds.width),
            Self::format_number(draw_list.bounds.height),
            body
        )))
    }

    fn write_filled_rounded_rectangle(
        body: &mut String,
        style: &DrawStyle,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
    ) -> PixuiResult<()> {
        let (brush, opacity_attribute) = Self::brush_attributes(&style.brush, "fill");
        write!(
            body,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\" {}{} />",
            Self::format_number(x),
            Self::format_number(y),
            Self::format_number(width),
            Self::format_number(height),
            Self::format_number(radius),
            Self::format_number(radius),
            brush,
            opacity_attribute,
        )
        .map_err(|_| err!("Failed to render filled rounded rectangle"))?;
        Ok(())
    }

    fn write_outlined_rounded_rectangle(
        body: &mut String,
        style: &DrawStyle,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
    ) -> PixuiResult<()> {
        let (brush, opacity_attribute) = Self::brush_attributes(&style.brush, "stroke");
        write!(
            body,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"none\" {} stroke-width=\"{}\"{} />",
            Self::format_number(x),
            Self::format_number(y),
            Self::format_number(width),
            Self::format_number(height),
            Self::format_number(radius),
            Self::format_number(radius),
            brush,
            Self::format_number(style.width),
            opacity_attribute,
        )
        .map_err(|_| err!("Failed to render outlined rounded rectangle"))?;
        Ok(())
    }

    fn brush_attributes(brush: &Brush, attribute_name: &str) -> (String, String) {
        match brush {
            Brush::SolidColor(color) => {
                let attribute = format!(
                    "{}=\"rgb({}, {}, {})\"",
                    attribute_name, color.red, color.green, color.blue
                );
                let opacity_attribute = if color.alpha == u8::MAX {
                    String::new()
                } else {
                    format!(
                        " {}-opacity=\"{}\"",
                        attribute_name,
                        Self::format_number(f32::from(color.alpha) / 255.0)
                    )
                };
                (attribute, opacity_attribute)
            }
        }
    }

    fn write_text(
        body: &mut String,
        style: &DrawStyle,
        x: f32,
        y: f32,
        text: &str,
    ) -> PixuiResult<()> {
        let (brush, opacity_attribute) = Self::brush_attributes(&style.brush, "fill");
        write!(
            body,
            "<text x=\"{}\" y=\"{}\" {}{} font-family=\"{}\" font-size=\"{}\">{}</text>",
            Self::format_number(x),
            Self::format_number(y),
            brush,
            opacity_attribute,
            Self::escape_attribute(style.text_style.font_family.as_str()),
            Self::format_number(style.text_style.font_size),
            Self::escape_text(text),
        )
        .map_err(|_| err!("Failed to render text"))?;
        Ok(())
    }

    fn escape_attribute(value: &str) -> String {
        let mut escaped = String::new();
        for character in value.chars() {
            match character {
                '&' => escaped.push_str("&amp;"),
                '"' => escaped.push_str("&quot;"),
                '<' => escaped.push_str("&lt;"),
                '>' => escaped.push_str("&gt;"),
                '\'' => escaped.push_str("&apos;"),
                _ => escaped.push(character),
            }
        }
        escaped
    }

    fn escape_text(value: &str) -> String {
        let mut escaped = String::new();
        for character in value.chars() {
            match character {
                '&' => escaped.push_str("&amp;"),
                '<' => escaped.push_str("&lt;"),
                '>' => escaped.push_str("&gt;"),
                _ => escaped.push(character),
            }
        }
        escaped
    }

    fn format_number(value: f32) -> String {
        let mut formatted = format!("{value:.3}");
        while formatted.contains('.') && formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
        if formatted == "-0" {
            String::from("0")
        } else {
            formatted
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SvgRenderer;
    use crate::draw::brush::Brush;
    use crate::draw::color::Color;
    use crate::draw::command::DrawCommand;
    use crate::draw::draw_bounds::DrawBounds;
    use crate::draw::draw_list::DrawList;
    use crate::draw::draw_style::DrawStyle;
    use crate::draw::text_style::TextStyle;
    use pixui_base::shared_string::SharedString;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn render_outputs_svg_for_a_small_validated_draw_list() {
        let mut draw_list = DrawList::new(DrawBounds::new(0.0, 0.0, 120.0, 80.0));
        let fill_style_id = draw_list.push_style(DrawStyle {
            brush: Brush::SolidColor(Color::rgba(10, 20, 30, 255)),
            width: 2.0,
            text_style: TextStyle::new("Inter", 14.0),
        });
        let outline_style_id = draw_list.push_style(DrawStyle {
            brush: Brush::SolidColor(Color::rgba(40, 50, 60, 128)),
            width: 4.0,
            text_style: TextStyle::new("Inter", 12.0),
        });

        draw_list.push_command(DrawCommand::SelectStyle {
            style_id: fill_style_id,
        });
        draw_list.push_command(DrawCommand::FillRoundedRectangle {
            x: 10.0,
            y: 12.0,
            width: 30.0,
            height: 20.0,
            radius: 6.0,
        });
        draw_list.push_command(DrawCommand::SelectStyle {
            style_id: outline_style_id,
        });
        draw_list.push_command(DrawCommand::OutlineRoundedRectangle {
            x: 50.0,
            y: 15.0,
            width: 25.0,
            height: 10.0,
            radius: 4.0,
        });
        draw_list.push_command(DrawCommand::SelectStyle {
            style_id: fill_style_id,
        });
        draw_list.push_command(DrawCommand::DrawText {
            x: 12.0,
            y: 52.0,
            text: SharedString::from("Save & Close"),
        });

        let svg = SvgRenderer::render(&draw_list).unwrap();

        assert_eq!(
            svg.as_str(),
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 120 80\" width=\"120\" height=\"80\"><rect x=\"10\" y=\"12\" width=\"30\" height=\"20\" rx=\"6\" ry=\"6\" fill=\"rgb(10, 20, 30)\" /><rect x=\"50\" y=\"15\" width=\"25\" height=\"10\" rx=\"4\" ry=\"4\" fill=\"none\" stroke=\"rgb(40, 50, 60)\" stroke-width=\"4\" stroke-opacity=\"0.502\" /><text x=\"12\" y=\"52\" fill=\"rgb(10, 20, 30)\" font-family=\"Inter\" font-size=\"14\">Save &amp; Close</text></svg>"
        );
    }

    #[test]
    fn render_requires_an_active_style_before_drawing() {
        let mut draw_list = DrawList::new(DrawBounds::new(0.0, 0.0, 100.0, 80.0));
        draw_list.push_command(DrawCommand::FillRoundedRectangle {
            x: 10.0,
            y: 12.0,
            width: 30.0,
            height: 20.0,
            radius: 6.0,
        });

        let error = SvgRenderer::render(&draw_list).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("requires an active style before drawing a filled rounded rectangle")
        );
    }

    #[test]
    fn render_rejects_unknown_style_ids() {
        let mut draw_list = DrawList::new(DrawBounds::new(0.0, 0.0, 100.0, 80.0));
        draw_list.push_command(DrawCommand::SelectStyle {
            style_id: crate::draw::style_id::StyleId::new(3),
        });

        let error = SvgRenderer::render(&draw_list).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("selected unknown style id 3")
        );
    }

    #[test]
    fn render_requires_an_active_style_before_drawing_text() {
        let mut draw_list = DrawList::new(DrawBounds::new(0.0, 0.0, 100.0, 80.0));
        draw_list.push_command(DrawCommand::DrawText {
            x: 10.0,
            y: 20.0,
            text: SharedString::from("Hello"),
        });

        let error = SvgRenderer::render(&draw_list).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("requires an active style before drawing text")
        );
    }

    #[test]
    fn write_showcase_svg_to_root_target_dir() {
        let draw_list = showcase_draw_list();
        let svg = SvgRenderer::render(&draw_list).unwrap();
        let output_path = showcase_output_path();

        fs::create_dir_all(output_path.parent().unwrap()).unwrap();
        fs::write(&output_path, svg.as_str()).unwrap();

        let written_svg = fs::read_to_string(&output_path).unwrap();
        assert_eq!(written_svg, svg.as_str());
    }

    fn showcase_draw_list() -> DrawList {
        let mut draw_list = DrawList::new(DrawBounds::new(0.0, 0.0, 360.0, 220.0));

        let label_style_id = draw_list.push_style(DrawStyle {
            brush: Brush::SolidColor(Color::rgba(226, 232, 240, 255)),
            width: 1.0,
            text_style: TextStyle::new("Inter", 14.0),
        });
        let fill_style_id = draw_list.push_style(DrawStyle {
            brush: Brush::SolidColor(Color::rgba(37, 99, 235, 255)),
            width: 2.0,
            text_style: TextStyle::new("Inter", 14.0),
        });
        let outline_style_id = draw_list.push_style(DrawStyle {
            brush: Brush::SolidColor(Color::rgba(249, 115, 22, 255)),
            width: 4.0,
            text_style: TextStyle::new("Inter", 14.0),
        });
        let text_style_id = draw_list.push_style(DrawStyle {
            brush: Brush::SolidColor(Color::rgba(168, 85, 247, 255)),
            width: 1.0,
            text_style: TextStyle::new("IBM Plex Sans", 24.0),
        });

        let margin = 20.0;
        let gap_x = 28.0;
        let gap_y = 26.0;
        let column_width = 146.0;
        let row_height = 74.0;
        let label_offset_y = 14.0;
        let box_offset_y = 28.0;
        let box_width = 132.0;
        let box_height = 32.0;

        for row in 0..2 {
            for column in 0..2 {
                let cell_x = margin + (column_width + gap_x) * column as f32;
                let cell_y = margin + (row_height + gap_y) * row as f32;

                let label = match (column, row) {
                    (0, 0) => "Filled rounded rectangle",
                    (1, 0) => "Outlined rounded rectangle",
                    (0, 1) => "Square corners",
                    (1, 1) => "Text drawing command",
                    _ => unreachable!(),
                };

                draw_list.push_command(DrawCommand::SelectStyle {
                    style_id: label_style_id,
                });
                draw_list.push_command(DrawCommand::DrawText {
                    x: cell_x,
                    y: cell_y + label_offset_y,
                    text: SharedString::from(label),
                });

                match (column, row) {
                    (0, 0) => {
                        draw_list.push_command(DrawCommand::SelectStyle {
                            style_id: fill_style_id,
                        });
                        draw_list.push_command(DrawCommand::FillRoundedRectangle {
                            x: cell_x,
                            y: cell_y + box_offset_y,
                            width: box_width,
                            height: box_height,
                            radius: 12.0,
                        });
                    }
                    (1, 0) => {
                        draw_list.push_command(DrawCommand::SelectStyle {
                            style_id: outline_style_id,
                        });
                        draw_list.push_command(DrawCommand::OutlineRoundedRectangle {
                            x: cell_x,
                            y: cell_y + box_offset_y,
                            width: box_width,
                            height: box_height,
                            radius: 12.0,
                        });
                    }
                    (0, 1) => {
                        draw_list.push_command(DrawCommand::SelectStyle {
                            style_id: fill_style_id,
                        });
                        draw_list.push_command(DrawCommand::FillRoundedRectangle {
                            x: cell_x,
                            y: cell_y + box_offset_y,
                            width: box_width,
                            height: box_height,
                            radius: 0.0,
                        });
                    }
                    (1, 1) => {
                        draw_list.push_command(DrawCommand::SelectStyle {
                            style_id: text_style_id,
                        });
                        draw_list.push_command(DrawCommand::DrawText {
                            x: cell_x,
                            y: cell_y + box_offset_y + 24.0,
                            text: SharedString::from("Hello SVG"),
                        });
                    }
                    _ => unreachable!(),
                }
            }
        }

        draw_list
    }

    fn showcase_output_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("target")
            .join("test-artifacts")
            .join("draw")
            .join("svg_renderer_showcase.svg")
    }
}
