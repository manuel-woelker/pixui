//! Font snapshots are self-contained and stable across batching and eviction.

use pixui_engine::ui::{
    display_list::{Color, DrawCommand},
    display_list_builder::DisplayListBuilder,
    geometry::Point,
    text::{
        font::{FontConfig, FontFace, normalize},
        resource::{FontIndex, FontMetrics, FontResource, GlyphAtlas, GlyphInfo, PixelRect},
        service::{TextLimits, TextService},
    },
};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

fn config(size: f32, scale: f32) -> FontConfig {
    FontConfig::new(FontFace::geist().unwrap(), size, scale).unwrap()
}
fn chars(text: &str) -> BTreeSet<char> {
    text.chars().collect()
}
fn frame(
    service: &mut TextService,
    lines: &[&str],
    color: Color,
    origin: Point,
) -> pixui_engine::ui::display_list::DisplayList {
    let mut builder = DisplayListBuilder::default();
    for line in lines {
        builder
            .text(config(16.0, 1.0), origin, *line, color)
            .unwrap();
    }
    builder.finish_with_text(service).unwrap().0
}
fn glyph_pixels(resource: &FontResource, character: char) -> Vec<u8> {
    let Some(rect) = resource.glyph(character).unwrap().atlas_rect else {
        return Vec::new();
    };
    let atlas = resource.atlas();
    (rect.y..rect.y + rect.height)
        .flat_map(|y| {
            let start = (y * atlas.width() + rect.x) as usize;
            atlas.coverage()[start..start + rect.width as usize]
                .iter()
                .copied()
        })
        .collect()
}

#[test]
fn painters_batch_into_one_resource_and_order_does_not_change_packing() {
    let mut service = TextService::default();
    let display = frame(
        &mut service,
        &["ABC", "CäB", "ABC"],
        Color(0, 0, 0),
        Point::default(),
    );
    assert_eq!(display.fonts.len(), 1);
    assert_eq!(service.rasterized_glyphs(), 4);
    assert_eq!(display.fonts[0].characters().len(), 4);
    assert!(display.commands.iter().all(|command| matches!(
        command,
        DrawCommand::DrawText {
            font: FontIndex(0),
            ..
        }
    )));
    let other = frame(
        &mut TextService::default(),
        &["CäB", "ABC"],
        Color(0, 0, 0),
        Point::default(),
    );
    assert_eq!(display.fonts, other.fonts);
    let next = frame(
        &mut service,
        &["ABC"],
        Color(255, 255, 255),
        Point { x: 400.0, y: 32.0 },
    );
    assert!(Arc::ptr_eq(&display.fonts[0], &next.fonts[0]));
    assert_eq!(service.rasterized_glyphs(), 4);
}

#[test]
fn spaces_controls_and_missing_characters_follow_one_measurement_policy() {
    assert_eq!(normalize("A\r\nB\rC\tD").unwrap(), "A\nB\nC    D");
    assert!(normalize("a\0b").is_err());
    let mut service = TextService::default();
    let resource = service
        .prepare(&config(16.0, 1.0), &chars(" 🦀🛰中"))
        .unwrap();
    assert_eq!(service.rasterized_glyphs(), 2); // space + single replacement
    assert_eq!(resource.glyph('🦀'), resource.glyph('中'));
    assert!(resource.glyph(' ').unwrap().advance > 0.0);
    assert_eq!(resource.glyph(' ').unwrap().atlas_rect, None);
    let added_alias = service.prepare(&config(16.0, 1.0), &chars("🙂")).unwrap();
    assert!(!Arc::ptr_eq(&resource, &added_alias));
    assert!(Arc::ptr_eq(resource.atlas(), added_alias.atlas()));
    assert_eq!(service.rasterized_glyphs(), 2);
    let mut builder = DisplayListBuilder::default();
    let measured = builder
        .measure_text(&config(16.0, 1.0), "A\tB\r\nA")
        .unwrap();
    builder
        .text(
            config(16.0, 1.0),
            Point::default(),
            "A\tB\r\nA",
            Color(0, 0, 0),
        )
        .unwrap();
    let display = builder.finish_with_text(&mut service).unwrap().0;
    let font = &display.fonts[0];
    assert_eq!(
        measured.width,
        font.glyph('A').unwrap().advance
            + 4.0 * font.glyph(' ').unwrap().advance
            + font.glyph('B').unwrap().advance
    );
    assert_eq!(measured.height, 2.0 * font.metrics().line_height);
    let empty = frame(
        &mut TextService::default(),
        &[""],
        Color(0, 0, 0),
        Point::default(),
    );
    empty.validate().unwrap();
    assert!(empty.fonts[0].characters().is_empty());
    assert!(service.prepare(&config(16.0, 1.0), &chars("\n")).is_err());
}

#[test]
fn growth_preserves_old_outputs_and_existing_coverage_without_rerasterization() {
    let mut service = TextService::new(TextLimits {
        initial_dimension: 16,
        maximum_dimension: 512,
        ..Default::default()
    })
    .unwrap();
    let first = service.prepare(&config(16.0, 1.0), &chars("A")).unwrap();
    let old_pixels = first.atlas().coverage().to_vec();
    let alphabet: BTreeSet<_> = (' '..='~').collect();
    let expanded = service.prepare(&config(16.0, 1.0), &alphabet).unwrap();
    assert!(expanded.atlas().width() > first.atlas().width());
    assert_eq!(first.atlas().coverage(), old_pixels);
    assert_eq!(glyph_pixels(&first, 'A'), glyph_pixels(&expanded, 'A'));
    assert_eq!(service.rasterized_glyphs(), alphabet.len() as u64);
    let again = service.prepare(&config(16.0, 1.0), &alphabet).unwrap();
    assert!(Arc::ptr_eq(&expanded, &again));
}

#[test]
fn configuration_identity_scale_and_lru_eviction_preserve_retained_frames() {
    let mut service = TextService::new(TextLimits {
        configurations: 2,
        ..Default::default()
    })
    .unwrap();
    let first = service.prepare(&config(16.0, 1.0), &chars("A")).unwrap();
    let weak = Arc::downgrade(&first);
    let other_size = service.prepare(&config(20.0, 1.0), &chars("A")).unwrap();
    let touched = service.prepare(&config(16.0, 1.0), &chars("A")).unwrap();
    assert!(Arc::ptr_eq(&first, &touched));
    let scaled = service.prepare(&config(16.0, 2.0), &chars("A")).unwrap();
    assert_eq!(scaled.scale(), 2.0);
    assert_eq!(service.cached_configurations(), 2);
    let refreshed = service.prepare(&config(20.0, 1.0), &chars("A")).unwrap();
    assert!(!Arc::ptr_eq(&other_size, &refreshed));
    assert_eq!(
        glyph_pixels(&other_size, 'A'),
        glyph_pixels(&refreshed, 'A')
    );
    assert!(weak.upgrade().is_some());
    drop(first);
    drop(touched);
    assert!(weak.upgrade().is_none()); // evicted cache + released frames
    let bytes = include_bytes!(concat!(
        env!("PIXUI_GEIST_DIRECTORY"),
        "/fonts/Geist/ttf/Geist-Regular.ttf"
    ));
    let separately_loaded =
        FontConfig::new(FontFace::from_bytes(bytes, 0).unwrap(), 20.0, 1.0).unwrap();
    let different = service.prepare(&separately_loaded, &chars("A")).unwrap();
    assert!(!Arc::ptr_eq(&different, &refreshed));
}

#[test]
fn exhausted_atlas_and_alias_limits_do_not_replace_usable_cache_entry() {
    let limits = TextLimits {
        initial_dimension: 32,
        maximum_dimension: 32,
        ..Default::default()
    };
    let mut service = TextService::new(limits).unwrap();
    let first = service.prepare(&config(16.0, 1.0), &chars("A")).unwrap();
    assert!(
        service
            .prepare(&config(16.0, 1.0), &(' '..='~').collect())
            .is_err()
    );
    assert_eq!(service.rasterized_glyphs(), 1); // impossible batch rejected before bitmap allocation
    assert!(Arc::ptr_eq(
        &first,
        &service.prepare(&config(16.0, 1.0), &chars("A")).unwrap()
    ));
    let mut service = TextService::new(TextLimits {
        characters_per_font: 1,
        ..Default::default()
    })
    .unwrap();
    let first = service.prepare(&config(16.0, 1.0), &chars("A")).unwrap();
    assert!(service.prepare(&config(16.0, 1.0), &chars("B")).is_err());
    assert!(Arc::ptr_eq(
        &first,
        &service.prepare(&config(16.0, 1.0), &chars("A")).unwrap()
    ));
    let mut builder = DisplayListBuilder::default();
    builder
        .text(config(16.0, 1.0), Point::default(), "ABC", Color(0, 0, 0))
        .unwrap();
    assert!(builder.finish_with_text(&mut service).is_err());
}

#[test]
fn metadata_fonts_and_commands_are_validated() {
    let bytes = include_bytes!(concat!(
        env!("PIXUI_GEIST_DIRECTORY"),
        "/fonts/Geist/ttf/Geist-Regular.ttf"
    ));
    assert!(FontFace::from_bytes(bytes, u32::MAX).is_err());
    assert!(FontFace::from_bytes(b"invalid", 0).is_err());
    for (size, scale) in [
        (0.0, 1.0),
        (f32::NAN, 1.0),
        (16.0, f32::INFINITY),
        (300.0, 1.0),
        (16.0, 0.0),
    ] {
        assert!(FontConfig::new(FontFace::geist().unwrap(), size, scale).is_err());
    }
    assert!(GlyphAtlas::new(0, 1, Vec::new()).is_err());
    assert!(GlyphAtlas::new(2049, 1, vec![0; 2049]).is_err());
    assert!(GlyphAtlas::new(1, 1, vec![0, 0]).is_err());
    let atlas = Arc::new(GlyphAtlas::new(1, 1, vec![128]).unwrap());
    let metrics = FontMetrics {
        ascent: 1.0,
        descent: 0.0,
        line_height: 1.0,
    };
    let glyph = GlyphInfo {
        advance: 1.0,
        offset: Point::default(),
        atlas_rect: Some(PixelRect {
            x: u32::MAX,
            y: 0,
            width: 1,
            height: 1,
        }),
    };
    assert!(FontResource::new(atlas.clone(), HashMap::from([('A', glyph)]), metrics, 1.0).is_err());
    assert!(
        FontResource::new(
            atlas,
            HashMap::new(),
            FontMetrics {
                ascent: f32::NAN,
                ..metrics
            },
            1.0
        )
        .is_err()
    );
    let mut display = frame(
        &mut TextService::default(),
        &["A"],
        Color(0, 0, 0),
        Point::default(),
    );
    if let DrawCommand::DrawText { text, .. } = &mut display.commands[0] {
        *text = "B".into();
    }
    assert!(display.validate().is_err());
    if let DrawCommand::DrawText { font, .. } = &mut display.commands[0] {
        *font = FontIndex(1);
    }
    assert!(display.validate().is_err());
    assert!(
        TextService::new(TextLimits {
            maximum_dimension: 4096,
            ..Default::default()
        })
        .is_err()
    );
}

#[test]
fn coverage_budget_evicts_cache_ownership_without_invalidating_snapshots() {
    let mut service = TextService::new(TextLimits {
        initial_dimension: 128,
        maximum_dimension: 128,
        coverage_bytes: 128 * 128,
        ..Default::default()
    })
    .unwrap();
    let retained = service.prepare(&config(16.0, 1.0), &chars("A")).unwrap();
    let weak = Arc::downgrade(&retained);
    service.prepare(&config(20.0, 1.0), &chars("B")).unwrap();
    assert_eq!(service.cached_configurations(), 1);
    assert_eq!(service.cached_coverage_bytes(), 128 * 128);
    assert!(!glyph_pixels(&retained, 'A').is_empty());
    drop(retained);
    assert!(weak.upgrade().is_none());
}

#[test]
fn appending_glyphs_keeps_coordinates_and_allocates_one_new_snapshot() {
    let mut service = TextService::default();
    let first = service.prepare(&config(16.0, 1.0), &chars("A")).unwrap();
    let next = service.prepare(&config(16.0, 1.0), &chars("BBAB")).unwrap();
    assert_eq!(first.glyph('A'), next.glyph('A'));
    assert_eq!(first.atlas().width(), next.atlas().width());
    assert!(!Arc::ptr_eq(first.atlas(), next.atlas()));
    assert!(first.glyph('B').is_none());
    assert_eq!(service.rasterized_glyphs(), 2);
    assert!(Arc::ptr_eq(
        &next,
        &service.prepare(&config(16.0, 1.0), &chars("AB")).unwrap()
    ));
}
