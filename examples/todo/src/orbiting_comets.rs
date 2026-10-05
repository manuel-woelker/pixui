//! A custom component generating a tiny color-keyed RGB snapshot each render.

use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    component_registry::component_id::ComponentId,
    expression::context::ExpressionContext,
    live_model::component::Component,
    painters::{context::PaintContext, painter::Painter, palette::Palette},
    ui::{
        display_list::Color,
        geometry::Rect,
        image::Image,
        presentation::{PresentationSettings, Theme},
    },
};
use std::f32::consts::{PI, TAU};

const WIDTH: u32 = 96;
const HEIGHT: u32 = 32;
const TRANSPARENT: Color = Color(255, 0, 255);

pub struct OrbitingComets;
pub struct CometProps {
    pub cyan: Color,
    pub orange: Color,
    pub speed: f32,
}
#[derive(Default)]
pub struct CometState;
impl Component for OrbitingComets {
    type Props = CometProps;
    type State = CometState;
}
pub struct CometPainter;

pub fn register(app: &ApplicationHandle) -> PixuiResult<ComponentId<OrbitingComets>> {
    let id = app.register_component::<OrbitingComets>("orbiting comets")?;
    app.register_painter::<OrbitingComets>(CometPainter)?;
    Ok(id)
}

pub fn props(
    _: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<CometProps> {
    Ok(CometProps {
        cyan: if settings.theme == Theme::Dark {
            Color(60, 230, 255)
        } else {
            Color(0, 125, 165)
        },
        orange: Color(240, 130, 35),
        speed: 1.0,
    })
}

impl Painter<OrbitingComets> for CometPainter {
    fn paint(&self, context: &mut PaintContext<'_, OrbitingComets>) -> PixuiResult<()> {
        // Reduce in f64 before converting to the pixel generator's phase, keeping
        // long-running timelines from losing their orbit in f32 precision.
        let seconds = context.timestamp_us as f64 / 1_000_000.0 * f64::from(context.props.speed);
        let phase = (seconds.rem_euclid(4.0) * f64::from(TAU) / 4.0) as f32;
        let image = frame(phase, context.props.cyan, context.props.orange)?;
        let scale = (context.width / WIDTH as f32)
            .min(context.height / HEIGHT as f32)
            .min(1.0);
        let width = WIDTH as f32 * scale;
        let height = HEIGHT as f32 * scale;
        context.fill_rect(
            context.bounds(),
            Palette::for_theme(context.settings.theme).control,
        );
        context.image(
            &image,
            Rect {
                x: (context.width - width) / 2.0,
                y: (context.height - height) / 2.0,
                width,
                height,
            },
        );
        if context.settings.timestamp_us.is_none() {
            context.request_animation_frame();
        }
        Ok(())
    }
}

/// Generate pixels at an explicit phase. Transparent gaps and dithered tails
/// reveal any background; there is no alpha channel or background-colored fade.
pub fn frame(phase: f32, cyan: Color, orange: Color) -> PixuiResult<Image> {
    if !phase.is_finite() || cyan == TRANSPARENT || orange == TRANSPARENT {
        return Err(pixui_error!(
            "invalid comet phase or reserved transparent color"
        ));
    }
    let phase = phase.rem_euclid(TAU);
    let mut pixels = vec![TRANSPARENT; (WIDTH * HEIGHT) as usize];
    let position = |angle: f32| (48.0 + 30.0 * angle.cos(), 16.0 + 10.0 * angle.sin());
    for dot in 0..40 {
        let (x, y) = position(dot as f32 * TAU / 40.0);
        pixels[y.round() as usize * WIDTH as usize + x.round() as usize] = Color(100, 110, 125);
    }
    for (angle, color) in [(phase, cyan), (phase + PI, orange)] {
        for tail in (0..10).rev() {
            let (cx, cy) = position(angle - tail as f32 * 0.14);
            let radius = 3.0 - tail as f32 * 0.22;
            // Only pixels inside this small box can belong to the dot. Scanning
            // the full image for every tail segment dominated frame generation.
            let left = (cx - radius).ceil().max(0.0) as u32;
            let right = (cx + radius).floor().min((WIDTH - 1) as f32) as u32;
            let top = (cy - radius).ceil().max(0.0) as u32;
            let bottom = (cy + radius).floor().min((HEIGHT - 1) as f32) as u32;
            for y in top..=bottom {
                for x in left..=right {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let covered = tail < 4 || (x + y) % 2 == 0;
                    if covered && dx * dx + dy * dy <= radius * radius {
                        pixels[(y * WIDTH + x) as usize] = color;
                    }
                }
            }
        }
        let (cx, cy) = position(angle);
        pixels[cy.round() as usize * WIDTH as usize + cx.round() as usize] = Color(255, 255, 220);
    }
    Image::new(WIDTH, HEIGHT, pixels, Some(TRANSPARENT))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_dot_scans_preserve_pixels_throughout_the_orbit() {
        let cyan = Color(10, 180, 230);
        let orange = Color(240, 130, 35);
        for step in 0..120 {
            let phase = step as f32 * TAU / 120.0;
            let mut expected = vec![TRANSPARENT; (WIDTH * HEIGHT) as usize];
            let position = |angle: f32| (48.0 + 30.0 * angle.cos(), 16.0 + 10.0 * angle.sin());
            for dot in 0..40 {
                let (x, y) = position(dot as f32 * TAU / 40.0);
                expected[y.round() as usize * WIDTH as usize + x.round() as usize] =
                    Color(100, 110, 125);
            }
            for (angle, color) in [(phase, cyan), (phase + PI, orange)] {
                for tail in (0..10).rev() {
                    let (cx, cy) = position(angle - tail as f32 * 0.14);
                    let radius = 3.0 - tail as f32 * 0.22;
                    for y in 0..HEIGHT {
                        for x in 0..WIDTH {
                            let dx = x as f32 - cx;
                            let dy = y as f32 - cy;
                            if (tail < 4 || (x + y) % 2 == 0)
                                && dx * dx + dy * dy <= radius * radius
                            {
                                expected[(y * WIDTH + x) as usize] = color;
                            }
                        }
                    }
                }
                let (cx, cy) = position(angle);
                expected[cy.round() as usize * WIDTH as usize + cx.round() as usize] =
                    Color(255, 255, 220);
            }
            assert_eq!(
                frame(phase, cyan, orange).unwrap().pixels(),
                expected,
                "phase {phase}"
            );
        }
    }

    #[test]
    fn master_timestamp_controls_comet_drawing_and_frozen_time_stops_requests() {
        use pixui_engine::{
            application::app::Application,
            live_model::{
                part::{ComponentPart, LivePart},
                state::LiveState,
            },
            ui::renderer,
        };
        let mut app = Application::default();
        let id = app.register_component::<OrbitingComets>("comets").unwrap();
        app.register_painter::<OrbitingComets>(CometPainter)
            .unwrap();
        let root = LivePart::Component(ComponentPart::typed(id, props));
        let mut state = LiveState::new();
        let mut draw = |timestamp_us| {
            let settings = PresentationSettings {
                timestamp_us,
                ..Default::default()
            };
            let rendered =
                renderer::render_measured(&root, &mut state, &app, &settings, 0.0, None, None)
                    .unwrap();
            (rendered.display_list.images[0].clone(), rendered.animating)
        };
        let (initial, delay) = draw(Some(0));
        assert!(!delay);
        assert_eq!(initial.pixels(), draw(Some(0)).0.pixels());
        assert_ne!(initial.pixels(), draw(Some(1_000_000)).0.pixels());
        assert_eq!(initial.pixels(), draw(Some(4_000_000)).0.pixels());
        assert_eq!(initial.pixels(), draw(Some(0)).0.pixels());
        assert!(draw(Some(u64::MAX)).0.pixels().contains(&TRANSPARENT));
        assert!(draw(None).1);
    }

    #[test]
    fn frames_are_deterministic_transparent_and_periodic() {
        let cyan = Color(10, 180, 230);
        let orange = Color(240, 130, 35);
        let initial = frame(0.0, cyan, orange).unwrap();
        let quarter = frame(PI / 2.0, cyan, orange).unwrap();
        assert_eq!(initial.width(), WIDTH);
        assert_eq!(initial.height(), HEIGHT);
        assert_eq!(initial.transparent_color(), Some(TRANSPARENT));
        assert!(initial.pixels().contains(&TRANSPARENT));
        assert!(initial.pixels().contains(&cyan));
        assert!(initial.pixels().contains(&orange));
        assert_ne!(initial.pixels(), quarter.pixels());
        assert_eq!(initial.pixels(), frame(TAU, cyan, orange).unwrap().pixels());
        assert_eq!(initial.pixels(), frame(0.0, cyan, orange).unwrap().pixels());
        assert_ne!(initial, frame(0.0, cyan, orange).unwrap());
        assert!(frame(f32::NAN, cyan, orange).is_err());
        assert!(frame(0.0, TRANSPARENT, orange).is_err());
    }
}
