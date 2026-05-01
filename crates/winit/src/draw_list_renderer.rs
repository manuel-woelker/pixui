use femtovg::renderer::OpenGl;
use femtovg::{Canvas, Color as FemtoColor, FontId, Paint, Path};
use pixui_base::err;
use pixui_base::result::{OptionExt, PixuiResult, ResultExt};
use pixui_base::shared_string::SharedString;
use pixui_engine::draw::brush::Brush;
use pixui_engine::draw::command::DrawCommand;
use pixui_engine::draw::draw_list::DrawList;
use pixui_engine::draw::draw_style::DrawStyle;
use pixui_engine::draw::style_id::StyleId;
use std::collections::HashMap;

/// Renders engine draw lists on a femtovg canvas.
#[derive(Default)]
pub struct DrawListRenderer {
    font_ids: HashMap<SharedString, FontId>,
}

impl DrawListRenderer {
    /// Executes a draw list on the provided canvas.
    pub fn render(&mut self, canvas: &mut Canvas<OpenGl>, draw_list: &DrawList) -> PixuiResult<()> {
        let mut active_style_id = None;

        for command in &draw_list.commands {
            match command {
                DrawCommand::SelectStyle { style_id } => {
                    active_style_id = Some(*style_id);
                }
                DrawCommand::FillRoundedRectangle {
                    x,
                    y,
                    width,
                    height,
                    radius,
                } => {
                    let style = current_style(&draw_list.styles, active_style_id)?;
                    let mut path = Path::new();
                    path.rounded_rect(*x, *y, *width, *height, *radius);
                    let paint = paint_from_style(style);
                    canvas.fill_path(&path, &paint);
                }
                DrawCommand::OutlineRoundedRectangle {
                    x,
                    y,
                    width,
                    height,
                    radius,
                } => {
                    let style = current_style(&draw_list.styles, active_style_id)?;
                    let mut path = Path::new();
                    path.rounded_rect(*x, *y, *width, *height, *radius);
                    let mut paint = paint_from_style(style);
                    paint.set_line_width(style.width);
                    canvas.stroke_path(&path, &paint);
                }
                DrawCommand::DrawText { x, y, text } => {
                    let style = current_style(&draw_list.styles, active_style_id)?;
                    let font_id =
                        self.font_id_for_family(canvas, style.text_style.font_family.as_ref())?;
                    let mut paint = paint_from_style(style);
                    paint.set_font(&[font_id]);
                    paint.set_font_size(style.text_style.font_size);
                    canvas
                        .fill_text(*x, *y, text.as_ref(), &paint)
                        .with_context(|| "failed to render draw-list text")?;
                }
            }
        }

        Ok(())
    }

    fn font_id_for_family(
        &mut self,
        canvas: &mut Canvas<OpenGl>,
        font_family: &str,
    ) -> PixuiResult<FontId> {
        if let Some(font_id) = self.font_ids.get(font_family) {
            return Ok(*font_id);
        }

        let font_id = load_font(canvas, font_family)?;
        self.font_ids.insert(font_family.into(), font_id);
        Ok(font_id)
    }
}

fn current_style(
    styles: &[DrawStyle],
    active_style_id: Option<StyleId>,
) -> PixuiResult<&DrawStyle> {
    let style_id = active_style_id.context("draw command requires an active style")?;
    styles
        .get(style_id.index())
        .with_context(|| format!("unknown style id {}", style_id.index()))
}

fn paint_from_style(style: &DrawStyle) -> Paint {
    let color = match &style.brush {
        Brush::SolidColor(color) => {
            FemtoColor::rgba(color.red, color.green, color.blue, color.alpha)
        }
    };
    Paint::color(color)
}

fn load_font(canvas: &mut Canvas<OpenGl>, font_family: &str) -> PixuiResult<FontId> {
    for candidate in font_candidates(font_family) {
        if let Ok(font_id) = canvas.add_font(candidate) {
            return Ok(font_id);
        }
    }

    Err(err!("no usable font file found for family `{font_family}`"))
}

fn font_candidates(font_family: &str) -> Vec<String> {
    let family = font_family.to_lowercase();
    let mut candidates = vec![
        format!("/usr/share/fonts/truetype/dejavu/{font_family}.ttf"),
        format!("/usr/share/fonts/truetype/liberation2/{font_family}.ttf"),
        format!("/Library/Fonts/{font_family}.ttf"),
        format!("C:/Windows/Fonts/{font_family}.ttf"),
    ];

    if family.contains("inter") {
        candidates.extend([
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".to_string(),
            "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf".to_string(),
            "/Library/Fonts/Arial.ttf".to_string(),
            "C:/Windows/Fonts/arial.ttf".to_string(),
        ]);
    }

    if family.contains("dejavu") {
        candidates.push("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".to_string());
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::current_style;
    use pixui_engine::draw::brush::Brush;
    use pixui_engine::draw::color::Color;
    use pixui_engine::draw::draw_style::DrawStyle;
    use pixui_engine::draw::style_id::StyleId;
    use pixui_engine::draw::text_style::TextStyle;

    fn style() -> DrawStyle {
        DrawStyle {
            brush: Brush::SolidColor(Color::rgba(255, 120, 64, 255)),
            width: 3.0,
            text_style: TextStyle::new("Inter", 14.0),
        }
    }

    #[test]
    fn current_style_requires_an_active_style() {
        let error = current_style(&[style()], None).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("draw command requires an active style")
        );
    }

    #[test]
    fn current_style_rejects_unknown_style_ids() {
        let error = current_style(&[style()], Some(StyleId::new(3))).unwrap_err();

        assert!(error.to_test_string().contains("unknown style id 3"));
    }
}
