//! One sampled timestamp per render, with explicit values for deterministic drawing.

use pixui_base::PixuiResult;
use pixui_engine::{
    application::app::Application,
    live_model::{
        component::Component,
        part::{ComponentPart, CompositePart, LivePart},
        state::LiveState,
    },
    painters::{context::PaintContext, painter::Painter},
    ui::{
        display_list::{Color, DrawCommand},
        geometry::Point,
        presentation::PresentationSettings,
        renderer,
    },
};

struct ClockComponent;
impl Component for ClockComponent {
    type Props = ();
    type State = ();
}
struct ClockPainter;
impl Painter<ClockComponent> for ClockPainter {
    fn measure(
        &self,
        context: &pixui_engine::painters::measure::MeasureContext<'_, ClockComponent>,
    ) -> PixuiResult<pixui_engine::ui::geometry::Size> {
        Ok(context.constrain(pixui_engine::ui::geometry::Size {
            width: 120.0,
            height: 36.0,
        }))
    }
    fn paint(&self, context: &mut PaintContext<'_, ClockComponent>) -> PixuiResult<()> {
        context.text(
            Point::default(),
            context.timestamp_us.to_string(),
            16.0,
            Color(1, 2, 3),
        )?;
        Ok(())
    }
}

#[test]
fn painters_share_one_timestamp_and_explicit_time_can_freeze_and_seek() {
    let mut app = Application::default();
    let id = app.register_component::<ClockComponent>("clock").unwrap();
    app.register_painter::<ClockComponent>(ClockPainter)
        .unwrap();
    let root = LivePart::Composite(CompositePart {
        parts: (0..2)
            .map(|_| LivePart::Component(ComponentPart::typed(id, |_, _| Ok(()))))
            .collect(),
    });
    let mut state = LiveState::new();
    let mut draw = |timestamp_us| {
        let settings = PresentationSettings {
            timestamp_us,
            ..Default::default()
        };
        let (display, _, _, _) =
            renderer::render(&root, &mut state, &app, &settings, 0.0, None, None).unwrap();
        let times: Vec<_> = display
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::DrawText { text, .. } => Some(text.parse::<u64>().unwrap()),
                _ => None,
            })
            .collect();
        assert_eq!(times.len(), 2);
        assert_eq!(times[0], times[1]);
        times[0]
    };
    for time in [0, 1_000_000, 1_000_000, 5, u64::MAX, 0] {
        assert_eq!(draw(Some(time)), time);
    }
    let automatic = draw(None);
    assert!(draw(None) >= automatic);
}
