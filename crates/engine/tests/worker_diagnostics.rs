//! Native-independent diagnostics: drawing, refresh and F11 policy live on worker.

use pixui_base::PixuiResult;
use pixui_engine::{
    application::app::Application,
    expression::context::ExpressionContext,
    live_model::{
        component::Component,
        part::{ComponentPart, LivePart},
    },
    painters::{context::PaintContext, painter::Painter},
    ui::{
        definition::UiDefinition,
        display_list::{Color, DrawCommand, RenderOutput, RenderRevision},
        input::{ButtonState, KeyboardEvent, UiCommand, UiInput},
        mailbox::OutputReceiver,
        performance::RendererTimings,
        presentation::PresentationSettings,
    },
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

struct Node;
impl Component for Node {
    type Props = ();
    type State = ();
}
struct CountingPainter(Arc<AtomicUsize>);
impl Painter<Node> for CountingPainter {
    fn measure(
        &self,
        context: &pixui_engine::painters::measure::MeasureContext<'_, Node>,
    ) -> PixuiResult<pixui_engine::ui::geometry::Size> {
        Ok(context.constrain(pixui_engine::ui::geometry::Size {
            width: 120.0,
            height: 36.0,
        }))
    }
    fn paint(&self, context: &mut PaintContext<'_, Node>) -> PixuiResult<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        context.fill_rect(context.bounds(), Color(1, 2, 3));
        Ok(())
    }
}
fn props(_: &ExpressionContext<'_>, _: &PresentationSettings) -> PixuiResult<()> {
    Ok(())
}
fn output(receiver: &OutputReceiver) -> RenderOutput {
    receiver.recv_timeout(Duration::from_secs(3)).unwrap()
}
fn overlay_text(output: &RenderOutput) -> &str {
    output
        .display_list
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::DrawText { text, .. } if text.starts_with("Performance (F11)") => {
                Some(text.as_str())
            }
            _ => None,
        })
        .expect("worker-generated performance text")
}

#[test]
fn worker_overlay_refreshes_without_repainting_or_counting_diagnostic_frames() {
    let app = Application::new();
    let paints = Arc::new(AtomicUsize::new(0));
    let component = app.register_component::<Node>("node").unwrap();
    app.register_painter::<Node>(CountingPainter(paints.clone()))
        .unwrap();
    let definition = app
        .register_ui(UiDefinition::new(
            "diagnostics",
            LivePart::Component(ComponentPart::typed(component, props)),
        ))
        .unwrap();
    let (id, receiver) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let (_, peer) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let initial = output(&receiver);
    output(&peer);
    assert_eq!(paints.load(Ordering::SeqCst), 2);
    let timings = RendererTimings {
        drawing: Duration::from_millis(2),
        ..Default::default()
    };
    app.ui_command(UiCommand::FramePresented {
        instance: id,
        paint_revision: initial.paint_revision,
        timings: Some(timings),
        timestamp: Instant::now(),
    })
    .unwrap();
    // F11 does not need geometry: even revision zero toggles the local overlay.
    app.ui_command(UiCommand::Input {
        instance: id,
        revision: RenderRevision(0),
        input: UiInput::Keyboard(KeyboardEvent::named("F11")),
    })
    .unwrap();
    let shown = output(&receiver);
    assert!(shown.revision > initial.revision);
    assert_eq!(shown.paint_revision, initial.paint_revision);
    assert_eq!(
        shown.display_list.commands.len(),
        initial.display_list.commands.len() + 5
    );
    shown.display_list.validate().unwrap();
    assert_eq!(paints.load(Ordering::SeqCst), 2);
    assert!(overlay_text(&shown).contains("1 fps"));
    assert!(overlay_text(&shown).contains("2.000 ms"));
    assert!(peer.try_recv().is_err());
    // Presenting a diagnostic output does not count as another application frame.
    app.ui_command(UiCommand::FramePresented {
        instance: id,
        paint_revision: shown.paint_revision,
        timings: Some(timings),
        timestamp: Instant::now(),
    })
    .unwrap();
    for (state, repeat) in [(ButtonState::Released, false), (ButtonState::Pressed, true)] {
        app.ui_command(UiCommand::Input {
            instance: id,
            revision: shown.revision,
            input: UiInput::Keyboard(KeyboardEvent {
                state,
                repeat,
                ..KeyboardEvent::named("F11")
            }),
        })
        .unwrap();
    }
    // No host tick or redraw command: the worker wakes itself for diagnostics.
    let refreshed = output(&receiver);
    assert!(refreshed.revision > shown.revision);
    assert_eq!(refreshed.paint_revision, initial.paint_revision);
    assert!(overlay_text(&refreshed).contains("1 fps"));
    assert_eq!(paints.load(Ordering::SeqCst), 2);
    // Hiding pauses diagnostic timers as well as component rendering.
    app.ui_command(UiCommand::Visibility {
        instance: id,
        visible: false,
    })
    .unwrap();
    app.inspect(|_| Ok(())).unwrap();
    while receiver.try_recv().is_ok() {}
    assert!(receiver.recv_timeout(Duration::from_millis(350)).is_err());
    assert_eq!(paints.load(Ordering::SeqCst), 2);
    app.ui_command(UiCommand::Visibility {
        instance: id,
        visible: true,
    })
    .unwrap();
    let restored = output(&receiver);
    assert!(overlay_text(&restored).starts_with("Performance"));
    assert_eq!(paints.load(Ordering::SeqCst), 3);
    app.ui_command(UiCommand::Input {
        instance: id,
        revision: initial.revision,
        input: UiInput::Keyboard(KeyboardEvent::named("F11")),
    })
    .unwrap();
    let hidden = output(&receiver);
    assert_eq!(hidden.display_list, initial.display_list);
    assert_eq!(hidden.paint_revision, restored.paint_revision);
    app.inspect(|_| Ok(())).unwrap();
    assert!(receiver.recv_timeout(Duration::from_millis(350)).is_err());
    assert_eq!(paints.load(Ordering::SeqCst), 3);
    assert!(
        app.ui_command(UiCommand::FramePresented {
            instance: id,
            paint_revision: RenderRevision(u64::MAX),
            timings: None,
            timestamp: Instant::now()
        })
        .is_err()
    );
    // Overlay refreshes and removal preserve compatible old hit-test geometry.
    app.ui_command(UiCommand::Input {
        instance: id,
        revision: initial.revision,
        input: UiInput::PointerMoved(Default::default()),
    })
    .unwrap();
}
