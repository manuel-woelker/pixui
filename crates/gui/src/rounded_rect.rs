//! Shared shape math for uniform rounded fills and inside borders.
use pixui_engine::ui::geometry::{Point, Rect};

pub(crate) fn radius(rect: Rect, requested: f32) -> f32 {
    requested.min(rect.width * 0.5).min(rect.height * 0.5)
}

pub(crate) fn inset(rect: Rect, width: f32) -> Rect {
    Rect {
        x: rect.x + width,
        y: rect.y + width,
        width: (rect.width - 2.0 * width).max(0.0),
        height: (rect.height - 2.0 * width).max(0.0),
    }
}

/// One physical pixel of edge coverage from the rounded rectangle's signed
/// distance. Interior pixels stay opaque; no multisampled framebuffer is needed.
pub(crate) fn coverage(rect: Rect, radius: f32, point: Point, scale: f32) -> f32 {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return 0.0;
    }
    let x = (point.x - rect.x - rect.width * 0.5).abs() - (rect.width * 0.5 - radius);
    let y = (point.y - rect.y - rect.height * 0.5).abs() - (rect.height * 0.5 - radius);
    let distance = (x.max(0.0).powi(2) + y.max(0.0).powi(2)).sqrt() + x.max(y).min(0.0) - radius;
    (0.5 - distance * scale).clamp(0.0, 1.0)
}
