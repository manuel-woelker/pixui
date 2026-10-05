//! One shared command sink, with indexed images and deferred font batches.

use super::{
    display_list::{DisplayList, DrawCommand},
    geometry::{Point, Size},
    image::{Image, ImageIdentity},
    text::{
        font::{FontConfig, FontKey, normalize},
        resource::FontIndex,
        service::TextService,
    },
};
use pixui_base::PixuiResult;
use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

/// An index into this display list's image table, not a global resource ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageIndex(pub usize);

/// The minimum painter-requested redraw interval, preventing zero-delay loops.
pub const MIN_REDRAW_DELAY: Duration = Duration::from_millis(1);

/// One render owns this builder. Commands are validated on finish; component
/// painters use PaintContext to translate and guard their local clip scope.
#[derive(Default)]
pub struct DisplayListBuilder {
    display: DisplayList,
    image_indices: HashMap<ImageIdentity, ImageIndex>,
    redraw_after: Option<Duration>,
    font_indices: HashMap<FontKey, FontIndex>,
    fonts: Vec<(FontConfig, BTreeSet<char>)>,
}

impl DisplayListBuilder {
    /// Register a font configuration and collect the union of required glyphs.
    /// Rasterization is deferred until finish. Origin is a baseline.
    pub fn text(
        &mut self,
        config: FontConfig,
        origin: Point,
        text: impl Into<String>,
        color: super::display_list::Color,
    ) -> PixuiResult<()> {
        let text = normalize(&text.into())?;
        let index = *self.font_indices.entry(config.key()).or_insert_with(|| {
            let index = FontIndex(self.fonts.len());
            self.fonts.push((config, BTreeSet::new()));
            index
        });
        self.fonts[index.0]
            .1
            .extend(text.chars().filter(|&character| character != '\n'));
        self.emit(DrawCommand::DrawText {
            origin,
            text,
            font: index,
            color,
        });
        Ok(())
    }
    /// Measures normalized, unkerned text without allocating coverage. Returns
    /// widest line and total line height; no wrapping or shaping is performed.
    pub fn measure_text(&self, config: &FontConfig, text: &str) -> PixuiResult<Size> {
        let text = normalize(text)?;
        let width = text
            .split('\n')
            .map(|line| {
                line.chars()
                    .map(|character| config.face.0.advance(character, config.size, config.scale))
                    .sum::<f32>()
            })
            .fold(0.0_f32, f32::max);
        Ok(Size {
            width,
            height: text.split('\n').count() as f32 * config.metrics().line_height,
        })
    }

    /// Owns one reference per distinct snapshot. Its allocation cannot be reused
    /// while the builder's image table retains that reference.
    pub fn image_index(&mut self, image: &Image) -> ImageIndex {
        *self
            .image_indices
            .entry(image.identity())
            .or_insert_with(|| {
                let index = ImageIndex(self.display.images.len());
                self.display.images.push(image.clone());
                index
            })
    }
    /// Append a command in final coordinates; finish validates geometry/resources.
    pub fn emit(&mut self, command: DrawCommand) {
        self.display.commands.push(command);
    }
    /// The shortest request wins. Excessively long requests are capped to one day.
    pub fn request_redraw_after(&mut self, delay: Duration) {
        let delay = delay.clamp(MIN_REDRAW_DELAY, Duration::from_secs(86_400));
        self.redraw_after = Some(
            self.redraw_after
                .map_or(delay, |earlier| earlier.min(delay)),
        );
    }
    /// Consumes the lookup map; snapshots stay owned by the completed display list.
    pub fn finish(self) -> PixuiResult<(DisplayList, Option<Duration>)> {
        self.finish_with_text(&mut TextService::default())
    }
    /// Finalize all demands against the worker cache before validating/publishing.
    /// No partial display list is returned when preparation fails.
    pub fn finish_with_text(
        mut self,
        service: &mut TextService,
    ) -> PixuiResult<(DisplayList, Option<Duration>)> {
        for (config, characters) in &self.fonts {
            self.display
                .fonts
                .push(service.prepare(config, characters)?);
        }
        self.display.validate()?;
        Ok((self.display, self.redraw_after))
    }
}
