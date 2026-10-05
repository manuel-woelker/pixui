//! Display-list translation without backend font parsing or framebuffer readback.
use super::{glyphs, texture_cache::TextureCache};
use femtovg::{Canvas, ImageFlags, ImageSource, Paint, Path, renderer::WGPURenderer};
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::ui::{
    display_list::{Color, DisplayList, DrawCommand},
    geometry::Rect,
    image::ImageData,
    text::resource::FontResource,
};

/// Per-window GPU resource ceiling. A single frame may temporarily exceed it.
pub const DEFAULT_CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub struct CacheStats {
    pub uploads: u64,
    pub textures: usize,
    pub bytes: usize,
}

pub(crate) struct Scene {
    pub canvas: Canvas<WGPURenderer>,
    images: TextureCache<ImageData>,
    fonts: TextureCache<FontResource>,
    frame: u64,
    budget: usize,
    max_texture: u32,
}
fn color(Color(r, g, b): Color) -> femtovg::Color {
    femtovg::Color::rgb(r, g, b)
}
fn path(rect: Rect) -> Path {
    let mut p = Path::new();
    p.rect(rect.x, rect.y, rect.width, rect.height);
    p
}
// Software solid fills cover every touched physical pixel. Snap their bounds
// outward so fractional DPI strokes retain the same thickness on the GPU.
fn solid_path(rect: Rect, scale: f32) -> Path {
    if rect.width == 0.0 || rect.height == 0.0 {
        return Path::new();
    }
    let left = (rect.x * scale).floor() / scale;
    let top = (rect.y * scale).floor() / scale;
    path(Rect {
        x: left,
        y: top,
        width: ((rect.x + rect.width) * scale).ceil() / scale - left,
        height: ((rect.y + rect.height) * scale).ceil() / scale - top,
    })
}
impl Scene {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, budget: usize) -> PixuiResult<Self> {
        let max_texture = device.limits().max_texture_dimension_2d;
        let canvas = Canvas::new(WGPURenderer::new(device, queue))
            .map_err(|e| pixui_error!("create femtovg canvas: {e}"))?;
        Ok(Self {
            canvas,
            images: Default::default(),
            fonts: Default::default(),
            frame: 0,
            budget,
            max_texture,
        })
    }
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            uploads: self.images.uploads + self.fonts.uploads,
            textures: self.images.len() + self.fonts.len(),
            bytes: self.images.bytes() + self.fonts.bytes(),
        }
    }
    fn texture_size(&self, width: u32, height: u32) -> PixuiResult<()> {
        if width > self.max_texture || height > self.max_texture {
            return Err(pixui_error!(
                "texture {width}x{height} exceeds GPU limit {}",
                self.max_texture
            ));
        }
        Ok(())
    }
    pub fn prepare(
        &mut self,
        display: &DisplayList,
        width: u32,
        height: u32,
        scale: f32,
    ) -> PixuiResult<()> {
        display.validate()?;
        self.frame = self.frame.wrapping_add(1);
        // Complete all uploads before recording commands, so upload errors do not
        // leave a partially recorded frame in the canvas.
        for image in display.images.iter() {
            self.texture_size(image.width(), image.height())?;
            if self.images.get(image, self.frame).is_none() {
                let pixels: Vec<_> = image
                    .pixels()
                    .iter()
                    .map(|&Color(r, g, b)| {
                        rgb::RGBA8::new(
                            r,
                            g,
                            b,
                            if image.transparent_color() == Some(Color(r, g, b)) {
                                0
                            } else {
                                255
                            },
                        )
                    })
                    .collect();
                let id = self
                    .canvas
                    .create_image(
                        ImageSource::Rgba(imgref::Img::new(
                            pixels.as_slice(),
                            image.width() as usize,
                            image.height() as usize,
                        )),
                        ImageFlags::NEAREST,
                    )
                    .map_err(|e| pixui_error!("upload image: {e}"))?;
                self.images.insert(image, id, pixels.len() * 4, self.frame);
            }
        }
        for font in display.fonts.iter() {
            let atlas = font.atlas();
            self.texture_size(atlas.width(), atlas.height())?;
            if self.fonts.get(font, self.frame).is_none() {
                let coverage: Vec<_> = atlas
                    .coverage()
                    .iter()
                    .map(|&c| rgb::alt::Gray(c))
                    .collect();
                let id = self
                    .canvas
                    .create_image(
                        ImageSource::Gray(imgref::Img::new(
                            coverage.as_slice(),
                            atlas.width() as usize,
                            atlas.height() as usize,
                        )),
                        ImageFlags::NEAREST,
                    )
                    .map_err(|e| pixui_error!("upload glyph coverage: {e}"))?;
                self.fonts.insert(font, id, coverage.len(), self.frame);
            }
        }
        self.canvas.set_size(width, height, 1.0);
        self.canvas.reset();
        self.canvas
            .clear_rect(0, 0, width, height, femtovg::Color::rgb(0, 0, 0));
        self.canvas.scale(scale, scale);
        self.canvas
            .scissor(0.0, 0.0, width as f32 / scale, height as f32 / scale);
        for command in &display.commands {
            match command {
                DrawCommand::FillRect { rect, color: c } => self.canvas.fill_path(
                    &solid_path(*rect, scale),
                    &Paint::color(color(*c)).with_anti_alias(false),
                ),
                DrawCommand::StrokeRect {
                    rect,
                    color: c,
                    width,
                } => {
                    // Existing strokes lie inside the rectangle rather than being
                    // centered on its boundary. Four strips preserve that contract.
                    let thickness = width.min(rect.width).min(rect.height);
                    for strip in [
                        Rect {
                            height: thickness,
                            ..*rect
                        },
                        Rect {
                            y: rect.y + rect.height - thickness,
                            height: thickness,
                            ..*rect
                        },
                        Rect {
                            width: thickness,
                            ..*rect
                        },
                        Rect {
                            x: rect.x + rect.width - thickness,
                            width: thickness,
                            ..*rect
                        },
                    ] {
                        self.canvas.fill_path(
                            &solid_path(strip, scale),
                            &Paint::color(color(*c)).with_anti_alias(false),
                        );
                    }
                }
                DrawCommand::PushClip { rect } => {
                    self.canvas.save();
                    self.canvas
                        .intersect_scissor(rect.x, rect.y, rect.width, rect.height);
                }
                DrawCommand::PopClip => self.canvas.restore(),
                DrawCommand::DrawImage {
                    image,
                    destination: r,
                } => {
                    let id = self
                        .images
                        .get(&display.images[*image], self.frame)
                        .expect("uploaded image");
                    self.canvas.fill_path(
                        &path(*r),
                        &Paint::image(id, r.x, r.y, r.width, r.height, 0.0, 1.0)
                            .with_anti_alias(false),
                    );
                }
                DrawCommand::DrawText {
                    origin,
                    text,
                    font,
                    color: c,
                } => {
                    let font = &display.fonts[*font];
                    let id = self.fonts.get(font, self.frame).expect("uploaded atlas");
                    let atlas = font.atlas();
                    // Bias by a tiny fraction of a texel toward floor's next
                    // texel at exact boundaries, avoiding interpolation roundoff
                    // selecting the previous coverage column at fractional DPI.
                    let quads = glyphs::positioned(font, *origin, text, scale)
                        .map(|(r, s)| femtovg::Quad {
                            x0: r.x,
                            y0: r.y,
                            x1: r.x + r.width,
                            y1: r.y + r.height,
                            s0: (s.x as f32 + 0.0001) / atlas.width() as f32,
                            t0: (s.y as f32 + 0.0001) / atlas.height() as f32,
                            s1: ((s.x + s.width) as f32 + 0.0001) / atlas.width() as f32,
                            t1: ((s.y + s.height) as f32 + 0.0001) / atlas.height() as f32,
                        })
                        .collect();
                    self.canvas.draw_glyph_commands(
                        femtovg::GlyphDrawCommands {
                            alpha_glyphs: vec![femtovg::DrawCommand {
                                image_id: id,
                                quads,
                            }],
                            color_glyphs: vec![],
                        },
                        &Paint::color(color(*c)),
                    );
                }
            }
        }
        Ok(())
    }
    /// Called after submitting the canvas commands. In-flight command buffers
    /// retain their GPU resources even when canvas cache handles are deleted.
    pub fn trim(&mut self) {
        for id in self.images.prune().into_iter().chain(self.fonts.prune()) {
            self.canvas.delete_image(id);
        }
        while self.stats().bytes > self.budget {
            let id = if self.fonts.oldest().is_none()
                || self.images.oldest() <= self.fonts.oldest() && self.images.oldest().is_some()
            {
                self.images.evict()
            } else {
                self.fonts.evict().or_else(|| self.images.evict())
            };
            let Some(id) = id else { break };
            self.canvas.delete_image(id);
        }
    }
}

#[cfg(test)]
mod tests;
