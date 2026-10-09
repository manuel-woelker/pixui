//! Logical caret geometry using exactly the renderer's unkerned font advances.
use super::editing::{Selection, validate};
use crate::ui::text::font::FontConfig;
use pixui_base::PixuiResult;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub struct CaretStop {
    pub byte: usize,
    pub x: f32,
}
#[derive(Clone, Debug)]
pub struct TextInputGeometry {
    pub stops: Vec<CaretStop>,
    pub inset: f32,
    pub line_height: f32,
}
impl TextInputGeometry {
    pub fn new(content: &str, config: &FontConfig, inset: f32) -> PixuiResult<Self> {
        validate(content)?;
        let mut stops = vec![CaretStop { byte: 0, x: 0.0 }];
        let mut x = 0.0;
        for (byte, grapheme) in content.grapheme_indices(true) {
            x += config.measure_text(grapheme)?.width;
            stops.push(CaretStop {
                byte: byte + grapheme.len(),
                x,
            });
        }
        Ok(Self {
            stops,
            inset,
            line_height: config.metrics().line_height,
        })
    }
    /// Reject invalid custom-painter geometry before it becomes an input target.
    pub fn validate(&self, content: &str) -> PixuiResult<()> {
        use pixui_base::pixui_error;
        if !self.inset.is_finite()
            || self.inset < 0.0
            || !self.line_height.is_finite()
            || self.line_height <= 0.0
            || self
                .stops
                .first()
                .is_none_or(|stop| stop.byte != 0 || stop.x != 0.0)
            || self
                .stops
                .last()
                .is_none_or(|stop| stop.byte != content.len())
            || self.stops.iter().any(|stop| {
                !stop.x.is_finite()
                    || stop.x < 0.0
                    || stop.byte > content.len()
                    || !content.is_char_boundary(stop.byte)
            })
            || !self
                .stops
                .iter()
                .map(|stop| stop.byte)
                .eq(std::iter::once(0).chain(
                    content
                        .grapheme_indices(true)
                        .map(|(byte, text)| byte + text.len()),
                ))
            || self
                .stops
                .windows(2)
                .any(|pair| pair[0].byte >= pair[1].byte || pair[0].x > pair[1].x)
        {
            return Err(pixui_error!("invalid input painter caret geometry"));
        }
        Ok(())
    }

    pub fn x(&self, byte: usize) -> f32 {
        self.stops
            .iter()
            .find(|stop| stop.byte == byte)
            .map_or(0.0, |stop| stop.x)
    }
    pub fn nearest(&self, x: f32) -> usize {
        self.stops
            .iter()
            .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
            .map_or(0, |stop| stop.byte)
    }
    pub fn reveal(&self, head: usize, width: f32, scroll: f32) -> f32 {
        let available = (width - 2.0 * self.inset - 1.0).max(0.0);
        let x = self.x(head);
        let maximum = (self.stops.last().map_or(0.0, |stop| stop.x) - available).max(0.0);
        if x < scroll {
            x
        } else if x > scroll + available {
            x - available
        } else {
            scroll
        }
        .clamp(0.0, maximum)
    }
}

/// Read-only interaction projected into every instance's painter.
#[derive(Clone, Debug)]
pub struct TextEditSnapshot {
    pub selection: Selection,
    pub geometry: TextInputGeometry,
    pub scroll: f32,
    pub blink_reset_us: u64,
    pub active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::text::font::FontFace;
    #[test]
    fn grapheme_stops_use_rendered_fallback_advances_and_width_specific_scroll() {
        for scale in [1.0, 1.5, 2.0] {
            let config = FontConfig::new(FontFace::geist().unwrap(), 16.0, scale).unwrap();
            let text = "á👩‍💻日本 W";
            let geometry = TextInputGeometry::new(text, &config, 8.0).unwrap();
            geometry.validate(text).unwrap();
            assert_eq!(geometry.stops.len(), text.graphemes(true).count() + 1);
            let end = geometry.x(text.len());
            assert!((end - config.measure_text(text).unwrap().width).abs() < 0.001);
            assert_eq!(geometry.nearest(-100.0), 0);
            assert_eq!(geometry.nearest(1000.0), text.len());
            assert!(geometry.reveal(text.len(), 40.0, 0.0) > 0.0);
            assert_eq!(geometry.reveal(text.len(), 300.0, 0.0), 0.0);
            let mut invalid = geometry.clone();
            invalid.stops.insert(1, CaretStop { byte: 1, x: 1.0 });
            assert!(
                invalid.validate(text).is_err(),
                "scalar boundary inside a grapheme"
            );
        }
    }
}
