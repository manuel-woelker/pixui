//! Offscreen compatibility fixture. Set PIXUI_REQUIRE_GPU=1 to require an adapter.
use super::*;
use crate::renderer::gpu::Gpu;
use pixui_engine::ui::{
    geometry::Point,
    image::Image,
    text::resource::{Font, FontMetrics, GlyphAtlas, GlyphInfo, PixelRect},
};
use std::{collections::HashMap, time::Duration};

fn gpu() -> Option<Gpu> {
    match Gpu::new(
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle()),
        None,
    ) {
        Ok(gpu) => Some(gpu),
        Err(error) if std::env::var_os("PIXUI_REQUIRE_GPU").is_none() => {
            eprintln!(
                "GPU test unavailable: {error:?}; set PIXUI_REQUIRE_GPU=1 to require validation"
            );
            None
        }
        Err(error) => panic!("required GPU adapter unavailable: {error:?}"),
    }
}
fn render(scene: &mut Scene, gpu: &Gpu, display: &DisplayList, scale: f32) -> Vec<u8> {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("renderer compatibility fixture"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    scene.prepare(display, 64, 64, scale).unwrap();
    gpu.queue.submit(scene.canvas.flush_to_output(&texture));
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fixture readback"),
        size: 64 * 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        tx.send(result).unwrap()
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
    let bytes = buffer.get_mapped_range(..).unwrap().to_vec();
    buffer.unmap();
    scene.trim();
    bytes
}
fn fixture() -> DisplayList {
    let image = Image::new(
        2,
        1,
        vec![Color(255, 0, 255), Color(40, 180, 20)],
        Some(Color(255, 0, 255)),
    )
    .unwrap();
    let atlas = GlyphAtlas::new(4, 2, vec![255, 128, 0, 0, 255, 128, 0, 0]).unwrap();
    let font = Font::from_value(
        FontResource::new(
            atlas,
            HashMap::from([
                (
                    'A',
                    GlyphInfo {
                        atlas_rect: Some(PixelRect {
                            x: 0,
                            y: 0,
                            width: 2,
                            height: 2,
                        }),
                        advance: 4.0,
                        offset: Point { x: 0.0, y: 0.0 },
                    },
                ),
                (
                    ' ',
                    GlyphInfo {
                        atlas_rect: None,
                        advance: 2.0,
                        offset: Point::default(),
                    },
                ),
            ]),
            FontMetrics {
                ascent: 2.0,
                descent: 0.0,
                line_height: 4.0,
            },
            1.0,
        )
        .unwrap(),
    );
    let mut display = DisplayList {
        images: vec![image.clone()].into(),
        fonts: vec![font.clone()].into(),
        commands: vec![],
    };
    display.commands.push(DrawCommand::FillRect {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
        },
        color: Color(20, 30, 40),
    });
    display.commands.push(DrawCommand::PushClip {
        rect: Rect {
            x: 1.0,
            y: 1.0,
            width: 30.0,
            height: 30.0,
        },
    });
    display.commands.push(DrawCommand::PushClip {
        rect: Rect {
            x: 2.0,
            y: 2.0,
            width: 25.0,
            height: 25.0,
        },
    });
    let image_index = pixui_engine::ui::resource_table::ResourceIndex::from_raw(0);
    display.commands.push(DrawCommand::DrawImage {
        image: image_index,
        destination: Rect {
            x: 4.0,
            y: 4.0,
            width: 8.0,
            height: 4.0,
        },
    });
    let index = pixui_engine::ui::resource_table::ResourceIndex::from_raw(0);
    display.commands.push(DrawCommand::DrawText {
        origin: Point { x: 3.0, y: 12.0 },
        text: "A A\nA".into(),
        font: index,
        color: Color(220, 80, 30),
    });
    display.commands.push(DrawCommand::StrokeRect {
        rect: Rect {
            x: 16.0,
            y: 3.0,
            width: 8.0,
            height: 8.0,
        },
        color: Color(30, 40, 200),
        width: 1.0,
    });
    display.commands.push(DrawCommand::PopClip);
    display.commands.push(DrawCommand::PopClip);
    display.commands.push(DrawCommand::PushClip {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        },
    });
    display.commands.push(DrawCommand::FillRect {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
        },
        color: Color(255, 0, 0),
    });
    display.commands.push(DrawCommand::PopClip);
    display.commands.push(DrawCommand::FillRect {
        rect: Rect {
            x: 32.25,
            y: 32.25,
            width: 0.0,
            height: 4.0,
        },
        color: Color(255, 0, 0),
    });
    display
}
#[test]
fn gpu_atlas_images_clips_dpi_and_cache_match_software() {
    let Some(gpu) = gpu() else { return };
    let mut scene = Scene::new(gpu.device.clone(), gpu.queue.clone(), DEFAULT_CACHE_BYTES).unwrap();
    let display = fixture();
    for scale in [1.0, 1.5, 2.0] {
        let pixels = render(&mut scene, &gpu, &display, scale);
        let cpu = crate::painter::paint(&display, 64, 64, scale).unwrap();
        // Integer geometry is exact. Fractional clip edges can be antialiased;
        // permit a small fraction of edge mismatches, never missing entire glyphs.
        let mismatches = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(cpu)
            .filter(|(rgba, pixel)| {
                (i16::from(rgba[0]) - ((pixel >> 16) & 255) as i16).abs() > 3
                    || (i16::from(rgba[1]) - ((pixel >> 8) & 255) as i16).abs() > 3
                    || (i16::from(rgba[2]) - (pixel & 255) as i16).abs() > 3
            })
            .count();
        assert!(
            mismatches <= 20,
            "scale {scale}: {mismatches} incorrect pixels"
        );
    }
    assert_eq!(scene.stats().uploads, 2);
    let reordered = display.clone();
    render(&mut scene, &gpu, &reordered, 1.0);
    assert_eq!(scene.stats().uploads, 2);
    drop(reordered);
    drop(display);
    scene.trim();
    assert_eq!(scene.stats().textures, 0);
}
#[test]
fn gpu_budget_evicts_live_snapshots_and_reuploads_them() {
    let Some(gpu) = gpu() else { return };
    let mut scene = Scene::new(gpu.device.clone(), gpu.queue.clone(), 0).unwrap();
    let display = fixture();
    render(&mut scene, &gpu, &display, 1.0);
    assert_eq!(scene.stats().textures, 0);
    render(&mut scene, &gpu, &display, 1.0);
    assert_eq!(scene.stats().uploads, 4);
}

#[test]
fn gpu_new_atlas_versions_and_changed_frame_indices_preserve_old_outputs() {
    let Some(gpu) = gpu() else { return };
    let mut scene = Scene::new(gpu.device.clone(), gpu.queue.clone(), DEFAULT_CACHE_BYTES).unwrap();
    let original = fixture();
    let pixels = render(&mut scene, &gpu, &original, 1.0);
    let mut changed = original.clone();
    changed.images = vec![original.images[0].clone(), original.images[0].clone()].into();
    for command in &mut changed.commands {
        if let DrawCommand::DrawImage { image, .. } = command {
            *image = pixui_engine::ui::resource_table::ResourceIndex::from_raw(1);
        }
    }
    assert_eq!(render(&mut scene, &gpu, &changed, 1.0), pixels);
    assert_eq!(scene.stats().uploads, 2);
    let old = &original.fonts[0];
    let atlas = GlyphAtlas::new(4, 2, vec![64, 128, 0, 0, 64, 128, 0, 0]).unwrap();
    let new = Font::from_value(
        FontResource::new(atlas, old.characters().clone(), old.metrics(), old.scale()).unwrap(),
    );
    changed.fonts = vec![new].into();
    assert_ne!(render(&mut scene, &gpu, &changed, 1.0), pixels);
    assert_eq!(scene.stats().uploads, 3);
    assert_eq!(render(&mut scene, &gpu, &original, 1.0), pixels);
    assert_eq!(scene.stats().uploads, 3);
    let mut invalid = original.clone();
    invalid.commands.push(DrawCommand::PopClip);
    assert!(scene.prepare(&invalid, 64, 64, 1.0).is_err());
    assert_eq!(render(&mut scene, &gpu, &original, 1.0), pixels);
}

/// Run explicitly in release mode; timings are informational, never a CI threshold.
#[test]
#[ignore = "explicit renderer profiling; requires an available GPU"]
fn profile_release_renderer_phases() {
    let gpu = gpu().expect("profiling requires a GPU");
    let mut scene = Scene::new(gpu.device.clone(), gpu.queue.clone(), DEFAULT_CACHE_BYTES).unwrap();
    let mut display = fixture();
    if let DrawCommand::FillRect { rect, .. } = &mut display.commands[0] {
        rect.width = 640.0;
        rect.height = 480.0;
    }
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("profiling target"),
        size: wgpu::Extent3d {
            width: 640,
            height: 480,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    // Warm up shader/pipeline creation before measuring steady-state frames.
    for _ in 0..5 {
        scene.prepare(&display, 640, 480, 1.0).unwrap();
        gpu.queue.submit(scene.canvas.flush_to_output(&texture));
        scene.trim();
    }
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let mut prepare = Duration::ZERO;
    let mut submit = Duration::ZERO;
    for _ in 0..100 {
        let start = std::time::Instant::now();
        scene.prepare(&display, 640, 480, 1.0).unwrap();
        prepare += start.elapsed();
        let start = std::time::Instant::now();
        gpu.queue.submit(scene.canvas.flush_to_output(&texture));
        submit += start.elapsed();
        scene.trim();
    }
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let start = std::time::Instant::now();
    for _ in 0..100 {
        std::hint::black_box(crate::painter::paint(&display, 640, 480, 1.0).unwrap());
    }
    eprintln!(
        "100 frames at 640x480: GPU prepare/upload {prepare:?}, CPU command submission {submit:?}, software rasterization {:?}, GPU cache {:?}",
        start.elapsed(),
        scene.stats()
    );
}

#[test]
fn gpu_budget_keeps_recent_textures_and_evicts_older_live_versions() {
    let Some(gpu) = gpu() else { return };
    let mut scene = Scene::new(gpu.device.clone(), gpu.queue.clone(), 16).unwrap();
    let older = fixture();
    render(&mut scene, &gpu, &older, 1.0);
    let mut newer = older.clone();
    newer.images =
        vec![Image::new(2, 1, vec![Color(0, 0, 0), Color(0, 0, 0)], None).unwrap()].into();
    render(&mut scene, &gpu, &newer, 1.0);
    assert_eq!(scene.stats().uploads, 3);
    assert_eq!(scene.stats().bytes, 16);
    render(&mut scene, &gpu, &newer, 1.0);
    assert_eq!(scene.stats().uploads, 3);
    render(&mut scene, &gpu, &older, 1.0);
    assert_eq!(scene.stats().uploads, 4);
}
