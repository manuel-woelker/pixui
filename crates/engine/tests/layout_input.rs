//! Layout and interaction agree even when controls are clipped or move beneath
//! a stationary pointer. The most recently active window owns shared hover.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::{
        app::Application, application_handle::ApplicationHandle,
        application_slice::ApplicationSlice,
    },
    expression::context::ExpressionContext,
    layout::{container::ContainerPart, style::LayoutStyle},
    live_model::{component::Component, part::ComponentPart},
    painters::{context::PaintContext, measure::MeasureContext, painter::Painter},
    ui::{
        activation::ActionBinding,
        definition::UiDefinition,
        display_list::{Color, DrawCommand, RenderOutput},
        geometry::{Point, Size},
        input::{ButtonState, KeyboardEvent, MouseButton, UiCommand, UiInput},
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
    },
};
use std::time::Duration;
#[pixui_engine::application::action::slice_actions(slice = "test", facade = Actions)]
mod actions {
    #[action]
    pub fn noop() {}
}
struct Node;
impl Component for Node {
    type Props = ();
    type State = ();
}
struct NodePainter;
impl Painter<Node> for NodePainter {
    fn measure(&self, context: &MeasureContext<'_, Node>) -> PixuiResult<Size> {
        if context.settings.locale == "invalid-measurement" {
            return Err(pixui_base::pixui_error!("measurement failed"));
        }
        Ok(context.constrain(Size {
            width: 40.0,
            height: 30.0,
        }))
    }
    fn paint(&self, context: &mut PaintContext<'_, Node>) -> PixuiResult<()> {
        context.fill_rect(
            context.bounds(),
            Color(
                if context.hovered { 255 } else { 0 },
                if context.focused { 255 } else { 0 },
                0,
            ),
        );
        Ok(())
    }
}
fn activate(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    let action = context
        .application()?
        .slice_named("test")?
        .action_handle_named("noop")?;
    Ok(Box::new(move |_| action.call(vec![])))
}
fn receive(outputs: &OutputReceiver) -> RenderOutput {
    outputs.recv_timeout(Duration::from_secs(2)).unwrap()
}
fn color(output: &RenderOutput, index: usize) -> Color {
    output
        .display_list
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::FillRect { color, .. } => Some(*color),
            _ => None,
        })
        .nth(index + 1)
        .unwrap()
}
fn setup() -> (
    ApplicationHandle,
    pixui_engine::ui::definition::UiDefinitionId,
) {
    let app = Application::new();
    let slice = app.add_slice(ApplicationSlice::new("test")).unwrap();
    actions::Actions::register(&app, slice).unwrap();
    let node = app.register_component::<Node>("node").unwrap();
    app.register_painter::<Node>(NodePainter).unwrap();
    let children = (0..3)
        .map(|_| {
            ComponentPart::typed(node, |_, _| Ok(()))
                .with_layout(LayoutStyle::fixed(100.0, 30.0).with_padding(4.0))
                .with_activation(activate)
                .into()
        })
        .collect();
    let root = ContainerPart::column()
        .with_layout(LayoutStyle::fixed(80.0, 45.0))
        .with_gap(5.0)
        .with_children(children)
        .into();
    let definition = app.register_ui(UiDefinition::new("input", root)).unwrap();
    (app, definition)
}
#[test]
fn clipped_pointer_targets_and_offscreen_keyboard_targets_are_separate() {
    let (app, definition) = setup();
    let (id, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let mut output = receive(&outputs);
    app.inspect(move |app| {
        let layout = app.uis().instance(id)?.layout();
        assert_eq!(layout.focus_targets.len(), 3);
        assert_eq!(layout.hit_regions.len(), 2);
        assert_eq!(layout.hit_regions[0].bounds.width, 80.0);
        assert_eq!(layout.hit_regions[1].bounds.height, 10.0);
        Ok(())
    })
    .unwrap();
    // Inside the child's untrimmed box, outside the enclosing container.
    app.ui_command(UiCommand::Input {
        instance: id,
        revision: output.revision,
        input: UiInput::PointerMoved(Point { x: 110.0, y: 20.0 }),
    })
    .unwrap();
    assert_eq!(
        app.inspect(move |app| Ok(app.uis().definition(definition)?.state().hover))
            .unwrap(),
        None
    );
    for index in 0..3 {
        app.ui_command(UiCommand::Input {
            instance: id,
            revision: output.revision,
            input: UiInput::Keyboard(KeyboardEvent::named("Tab")),
        })
        .unwrap();
        output = receive(&outputs);
        assert_eq!(color(&output, index), Color(0, 255, 0));
    }
    // The fully clipped third control can still be activated with the keyboard.
    app.ui_command(UiCommand::Input {
        instance: id,
        revision: output.revision,
        input: UiInput::Keyboard(KeyboardEvent::named("Enter")),
    })
    .unwrap();
    receive(&outputs);
    // Border-box padding is clickable, even outside the painter's content box.
    let revision = app
        .inspect(move |app| Ok(app.uis().instance(id)?.revision()))
        .unwrap();
    app.ui_command(UiCommand::Input {
        instance: id,
        revision,
        input: UiInput::MouseButton {
            button: MouseButton::Left,
            state: ButtonState::Released,
            position: Point { x: 17.0, y: 17.0 },
            modifiers: Default::default(),
        },
    })
    .unwrap();
    receive(&outputs);
}
#[test]
fn stationary_hover_recomputes_after_resize_and_peer_relayout_cannot_steal_it() {
    let (app, definition) = setup();
    let (one, a) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let (two, b) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let initial = receive(&a);
    receive(&b);
    app.ui_command(UiCommand::Input {
        instance: one,
        revision: initial.revision,
        input: UiInput::PointerMoved(Point { x: 30.0, y: 20.0 }),
    })
    .unwrap();
    let hovered = receive(&a);
    receive(&b);
    assert_eq!(color(&hovered, 0), Color(255, 0, 0));
    // Moving the peer's viewport edge does not change the active source's hover.
    app.ui_command(UiCommand::Present {
        instance: two,
        settings: PresentationSettings {
            viewport: Size {
                width: 20.0,
                height: 100.0,
            },
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(color(&receive(&b), 0), Color(255, 0, 0));
    let original_bounds = app
        .inspect(move |app| Ok(app.uis().instance(one)?.layout().component_bounds.clone()))
        .unwrap();
    app.ui_command(UiCommand::Present {
        instance: one,
        settings: PresentationSettings {
            viewport: Size {
                width: 20.0,
                height: 100.0,
            },
            ..Default::default()
        },
    })
    .unwrap();
    let resized = receive(&a);
    assert_eq!(
        app.inspect(move |app| Ok(app.uis().instance(one)?.layout().component_bounds.clone()))
            .unwrap(),
        original_bounds,
        "this resize changes clipping without moving fixed child boxes"
    );
    assert_eq!(color(&resized, 0), Color(0, 0, 0));
    assert_eq!(color(&receive(&b), 0), Color(0, 0, 0));
    assert!(
        app.ui_command(UiCommand::Input {
            instance: one,
            revision: hovered.revision,
            input: UiInput::Keyboard(KeyboardEvent::named("Tab"))
        })
        .is_err()
    );
    assert_eq!(
        app.inspect(move |app| Ok(app.uis().definition(definition)?.state().hover))
            .unwrap(),
        None
    );
}

#[test]
fn failed_measurement_keeps_last_successful_geometry_and_output_revision() {
    let (app, definition) = setup();
    let (id, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let initial = receive(&outputs);
    let bounds = app
        .inspect(move |app| Ok(app.uis().instance(id)?.layout().component_bounds.clone()))
        .unwrap();
    app.ui_command(UiCommand::Present {
        instance: id,
        settings: PresentationSettings {
            locale: "invalid-measurement".into(),
            ..Default::default()
        },
    })
    .unwrap();
    app.inspect(move |app| {
        let instance = app.uis().instance(id)?;
        assert_eq!(instance.revision(), initial.revision);
        assert_eq!(instance.layout().component_bounds, bounds);
        assert!(
            instance
                .last_error()
                .unwrap()
                .contains("measurement failed")
        );
        Ok(())
    })
    .unwrap();
    assert!(outputs.try_recv().is_err());
    app.ui_command(UiCommand::Present {
        instance: id,
        settings: PresentationSettings::default(),
    })
    .unwrap();
    assert!(receive(&outputs).revision > initial.revision);
}
