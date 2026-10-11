//! One shared command sink, with indexed images and deferred font batches.

use super::{
    display_list::{DisplayList, DrawCommand},
    geometry::{Point, Size},
    image::{Image, ImageData},
    resource_table::{ResourceIndex, ResourceTableBuilder},
    text::{
        font::{FontConfig, FontKey, normalize},
        resource::{FontIndex, FontResource},
        service::TextService,
    },
};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

/// An index into this display list's image table, not a global resource ID.
pub type ImageIndex = ResourceIndex<ImageData>;

/// The minimum painter-requested redraw interval, preventing zero-delay loops.
pub const MIN_REDRAW_DELAY: Duration = Duration::from_millis(1);

/// One render owns this builder. Commands are validated on finish; component
/// painters use PaintContext to translate and guard their local clip scope.
#[derive(Default)]
pub struct DisplayListBuilder {
    display: DisplayList,
    images: ResourceTableBuilder<ImageData>,
    redraw_after: Option<Duration>,
    animating: bool,
    font_indices: HashMap<FontKey, FontIndex>,
    styles: Vec<super::render_input::RenderStyle>,
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
            let index = FontIndex::from_raw(self.fonts.len());
            self.fonts.push((config, BTreeSet::new()));
            index
        });
        self.fonts[index.as_usize()]
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
    /// Install styles before painting. Each text slot acquires its final font
    /// index only when used, so decorative slots allocate no glyph atlas.
    pub(crate) fn set_styles(&mut self, styles: &[crate::styles::style::PaintStyle]) {
        self.styles = styles
            .iter()
            .map(|style| super::render_input::RenderStyle {
                fill: style.fill,
                border: style.border,
                foreground: style.foreground,
                font: None,
            })
            .collect();
    }
    pub fn styled_text(
        &mut self,
        config: FontConfig,
        origin: Point,
        text: impl Into<String>,
        style: crate::styles::index::StyleIndex,
    ) -> PixuiResult<()> {
        if self.styles.get(style.index).is_none() {
            return Err(pixui_error!("unbound text style"));
        }
        self.text(config, origin, text, super::display_list::Color(0, 0, 0))?;
        let Some(DrawCommand::DrawText {
            origin, text, font, ..
        }) = self.display.commands.pop()
        else {
            unreachable!("text appends one command")
        };
        let entry = &mut self.styles[style.index];
        if entry.font.is_some_and(|previous| previous != font) {
            return Err(pixui_error!("style used with inconsistent font"));
        }
        entry.font = Some(font);
        self.emit(DrawCommand::StyledText {
            origin,
            text,
            style,
        });
        Ok(())
    }
    /// Measures normalized, unkerned text without allocating coverage. Returns
    /// widest line and total line height; no wrapping or shaping is performed.
    pub fn measure_text(&self, config: &FontConfig, text: &str) -> PixuiResult<Size> {
        config.measure_text(text)
    }

    /// Owns one reference per distinct snapshot. Its allocation cannot be reused
    /// while the builder's image table retains that reference.
    pub fn image_index(&mut self, image: &Image) -> ImageIndex {
        self.images.insert(image)
    }
    /// Append a command in final coordinates; finish validates geometry/resources.
    pub fn emit(&mut self, command: DrawCommand) {
        self.display.commands.push(command);
    }
    /// Requests another frame at the native host's next drawing opportunity.
    /// Multiple painters combine their requests; headless consumers start no loop.
    pub fn request_animation_frame(&mut self) {
        self.animating = true;
    }
    pub(crate) fn animating(&self) -> bool {
        self.animating
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
        self,
        service: &mut TextService,
    ) -> PixuiResult<(DisplayList, Option<Duration>)> {
        let (display, styles, delay) = self.finish_styled(service)?;
        if !styles.is_empty() {
            return Err(pixui_error!(
                "styled lists require finish_styled to retain their table"
            ));
        }
        Ok((display, delay))
    }
    /// Finalizes glyphs and returns commands together with their owning style
    /// table. Equal renderer styles deduplicate; command indices are remapped.
    /// Errors publish neither table.
    pub fn finish_styled(
        mut self,
        service: &mut TextService,
    ) -> PixuiResult<(
        DisplayList,
        Vec<super::render_input::RenderStyle>,
        Option<Duration>,
    )> {
        let mut fonts = ResourceTableBuilder::<FontResource>::default();
        let mut indices = Vec::with_capacity(self.fonts.len());
        for (config, characters) in &self.fonts {
            indices.push(fonts.insert(&service.prepare(config, characters)?));
        }
        // Painting uses demand indices. Finalization translates them to shared
        // table indices, allowing snapshot deduplication to change table order.
        for command in &mut self.display.commands {
            if let DrawCommand::DrawText { font, .. } = command {
                *font = *indices
                    .get(font.as_usize())
                    .ok_or_else(|| pixui_error!("font index outside pending demands"))?;
            }
        }
        for style in &mut self.styles {
            if let Some(font) = &mut style.font {
                *font = *indices
                    .get(font.as_usize())
                    .ok_or_else(|| pixui_error!("style font outside pending demands"))?;
            }
        }
        // Metrics have already been consumed by layout. Deduplicate the concrete
        // renderer values, then remap output indices; painter indices never escape.
        let mut lookup = HashMap::new();
        let mut remap = Vec::with_capacity(self.styles.len());
        let mut styles = Vec::new();
        for style in self.styles.drain(..) {
            let index = *lookup.entry(style.key()).or_insert_with(|| {
                let index = crate::styles::index::StyleIndex {
                    index: styles.len(),
                };
                styles.push(style);
                index
            });
            remap.push(index);
        }
        for command in &mut self.display.commands {
            if let DrawCommand::StyledFillRect { style, .. }
            | DrawCommand::StyledStrokeRect { style, .. }
            | DrawCommand::StyledText { style, .. } = command
            {
                *style = *remap
                    .get(style.index)
                    .ok_or_else(|| pixui_error!("unbound drawing style"))?;
            }
        }
        self.styles = styles;
        self.display.fonts = fonts.finish();
        self.display.images = self.images.finish();
        self.display.validate_with_styles(&self.styles)?;
        Ok((self.display, self.styles, self.redraw_after))
    }
}
