/// Viewport dimensions used when rendering a component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Logical viewport width.
    pub width: f32,
    /// Logical viewport height.
    pub height: f32,
    /// Window scale factor used by the target surface.
    pub scale_factor: f32,
}

impl Viewport {
    /// Creates a viewport with explicit dimensions and scale factor.
    pub fn new(width: f32, height: f32, scale_factor: f32) -> Self {
        Self {
            width,
            height,
            scale_factor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Viewport;

    #[test]
    fn new_stores_explicit_dimensions() {
        let viewport = Viewport::new(640.0, 480.0, 2.0);

        assert_eq!(viewport.width, 640.0);
        assert_eq!(viewport.height, 480.0);
        assert_eq!(viewport.scale_factor, 2.0);
    }
}
