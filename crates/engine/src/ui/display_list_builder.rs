//! One command sink per render, with image deduplication by snapshot identity.

use super::{
    display_list::{DisplayList, DrawCommand},
    image::{Image, ImageIdentity},
};
use pixui_base::PixuiResult;
use std::{collections::HashMap, time::Duration};

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
}

impl DisplayListBuilder {
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
        self.display.validate()?;
        Ok((self.display, self.redraw_after))
    }
}
