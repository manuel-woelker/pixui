//! Frame diagnostics. Memory estimates describe referenced CPU data, not process RSS.
use super::display_list::{DisplayList, DrawCommand};
use std::{collections::HashSet, mem::size_of, time::Duration};

/// Disjoint worker CPU stages. Text includes atlas preparation and list validation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkerTimings {
    pub preparation: Duration,
    pub painting: Duration,
    pub text: Duration,
}

/// Last successful native frame's CPU timings; submission is not GPU execution.
/// Hosts report these to the application, which renders performance diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RendererTimings {
    pub acquisition: Duration,
    /// Resource conversion, texture allocation and upload recording.
    pub resources: Duration,
    pub drawing: Duration,
    pub submission: Duration,
}

/// Referenced payload estimates, deduplicated by snapshot identity within a frame.
/// Excludes allocator overhead, parsed faces, worker caches and GPU textures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameMemory {
    pub display_list: usize,
    pub images: usize,
    pub fonts: usize,
}
impl FrameMemory {
    pub fn measure(display: &DisplayList) -> Self {
        let mut result = Self {
            display_list: size_of::<DisplayList>()
                + display.commands.capacity() * size_of::<DrawCommand>()
                + display.images.storage_bytes()
                + display.fonts.storage_bytes(),
            ..Self::default()
        };
        for command in &display.commands {
            if let DrawCommand::DrawText { text, .. } = command {
                result.display_list += text.capacity();
            }
        }
        let mut images = HashSet::new();
        for image in display.images.iter() {
            if images.insert(image.identity()) {
                result.images += image.storage_bytes();
            }
        }
        let mut fonts = HashSet::new();
        let mut atlases = HashSet::new();
        for font in display.fonts.iter() {
            if fonts.insert(font.identity()) {
                result.fonts += size_of::<super::text::resource::FontResource>()
                    + font.characters().capacity()
                        * size_of::<(char, super::text::resource::GlyphInfo)>();
            }
            if atlases.insert(font.atlas().identity()) {
                result.fonts += font.atlas().storage_bytes();
            }
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{display_list::Color, image::Image};
    #[test]
    fn shared_font_snapshots_and_atlases_are_counted_once() {
        use crate::ui::text::resource::{Font, FontMetrics, FontResource, GlyphAtlas};
        let atlas = GlyphAtlas::new(2, 2, vec![255; 4]).unwrap();
        let make_font = || {
            Font::from_value(
                FontResource::new(
                    atlas.clone(),
                    Default::default(),
                    FontMetrics {
                        ascent: 10.0,
                        descent: -2.0,
                        line_height: 14.0,
                    },
                    1.0,
                )
                .unwrap(),
            )
        };
        let first = make_font();
        let second = make_font();
        let single = DisplayList {
            fonts: vec![first.clone()].into(),
            ..Default::default()
        };
        let repeated = DisplayList {
            fonts: vec![first.clone(), first, second].into(),
            ..Default::default()
        };
        assert_eq!(
            FrameMemory::measure(&single).fonts,
            size_of::<FontResource>() + atlas.storage_bytes()
        );
        assert_eq!(
            FrameMemory::measure(&repeated).fonts,
            2 * size_of::<FontResource>() + atlas.storage_bytes()
        );
    }
    #[test]
    fn repeated_handles_count_pixels_once_and_text_storage_is_included() {
        let image = Image::new(2, 1, vec![Color(0, 0, 0); 2], None).unwrap();
        let expected = image.storage_bytes();
        let mut display = DisplayList {
            images: vec![image.clone(), image].into(),
            ..Default::default()
        };
        let before = FrameMemory::measure(&display);
        assert_eq!(before.images, expected);
        display.commands.push(DrawCommand::DrawText {
            origin: Default::default(),
            text: "hello".into(),
            font: super::super::text::resource::FontIndex::from_raw(0),
            color: Color(0, 0, 0),
        });
        assert!(FrameMemory::measure(&display).display_list >= before.display_list + 5);
    }
}
