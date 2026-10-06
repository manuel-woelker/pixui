//! Shared definition interaction with independently sized and themed windows.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::{
        app::Application, application_handle::ApplicationHandle,
        application_slice::ApplicationSlice,
    },
    expression::context::ExpressionContext,
    live_model::{
        component::Component,
        part::{ComponentPart, CompositePart, LivePart},
    },
    painters::{context::PaintContext, painter::Painter},
    ui::{
        activation::ActionBinding,
        definition::UiDefinition,
        display_list::{Color, DrawCommand, RenderOutput},
        geometry::{Point, Size},
        input::{KeyboardEvent, UiCommand, UiInput, WheelDelta},
        mailbox::OutputReceiver,
        presentation::{PresentationSettings, Theme},
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
fn props(_: &ExpressionContext<'_>, _: &PresentationSettings) -> PixuiResult<()> {
    Ok(())
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
fn setup(
    count: usize,
) -> (
    ApplicationHandle,
    pixui_engine::ui::definition::UiDefinitionId,
) {
    let app = Application::new();
    let slice = app.add_slice(ApplicationSlice::new("test")).unwrap();
    actions::Actions::register(&app, slice).unwrap();
    let component = app.register_component::<Node>("node").unwrap();
    app.register_painter::<Node>(NodePainter).unwrap();
    let parts = (0..count)
        .map(|index| {
            let part = ComponentPart::typed(component, props);
            LivePart::Component(if index == 1 {
                part.with_activation(activate)
            } else {
                part
            })
        })
        .collect();
    let definition = app
        .register_ui(UiDefinition::new(
            "shared",
            LivePart::Composite(CompositePart { parts }),
        ))
        .unwrap();
    (app, definition)
}
fn output(receiver: &OutputReceiver) -> RenderOutput {
    receiver.recv_timeout(Duration::from_secs(2)).unwrap()
}
fn colors(output: &RenderOutput) -> Vec<Color> {
    output
        .display_list
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::FillRect { color, .. } => Some(*color),
            _ => None,
        })
        .skip(1)
        .collect()
}
#[test]
fn every_component_can_hover_and_focus_is_shared_but_other_definitions_are_isolated() {
    let (app, definition) = setup(3);
    let (first, a) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let (second, b) = app
        .create_ui(
            definition,
            PresentationSettings {
                theme: Theme::Dark,
                ..Default::default()
            },
        )
        .unwrap();
    let unrelated = app
        .register_ui(UiDefinition::new(
            "unrelated",
            LivePart::Component(ComponentPart::default()),
        ))
        .unwrap();
    let (_, other) = app
        .create_ui(unrelated, PresentationSettings::default())
        .unwrap();
    let initial = output(&a);
    output(&b);
    output(&other);
    app.ui_command(UiCommand::Input {
        instance: first,
        revision: initial.revision,
        input: UiInput::PointerMoved(Point { x: 20.0, y: 20.0 }),
    })
    .unwrap();
    let hovered = output(&a);
    assert_eq!(colors(&hovered)[0], Color(255, 0, 0));
    assert_eq!(colors(&output(&b))[0], Color(255, 0, 0));
    assert_eq!(
        hovered.display_list.commands.len(),
        initial.display_list.commands.len()
    );
    app.ui_command(UiCommand::Input {
        instance: first,
        revision: hovered.revision,
        input: UiInput::Keyboard(KeyboardEvent::named("Tab")),
    })
    .unwrap();
    let focused = output(&a);
    let peer = output(&b);
    assert_eq!(colors(&focused)[1], Color(0, 255, 0));
    assert_eq!(colors(&peer)[1], Color(0, 255, 0));
    assert_eq!(
        focused.display_list.commands.len(),
        initial.display_list.commands.len()
    );
    assert!(other.try_recv().is_err());
    // Hovering the third, noninteractive component in the other window wins.
    app.ui_command(UiCommand::Input {
        instance: second,
        revision: peer.revision,
        input: UiInput::PointerMoved(Point { x: 20.0, y: 110.0 }),
    })
    .unwrap();
    assert_eq!(colors(&output(&a))[2], Color(255, 0, 0));
    assert_eq!(colors(&output(&b))[2], Color(255, 0, 0));
    let state = app
        .inspect(move |app| Ok(app.uis().definition(definition)?.state().clone()))
        .unwrap();
    assert_eq!(state.hover, Some(2));
    assert_eq!(state.focus, Some(1));
    app.ui_command(UiCommand::Visibility {
        instance: second,
        visible: false,
    })
    .unwrap();
    let revision = app
        .inspect(move |app| Ok(app.uis().instance(first)?.revision()))
        .unwrap();
    app.ui_command(UiCommand::Input {
        instance: first,
        revision,
        input: UiInput::PointerMoved(Point { x: 20.0, y: 20.0 }),
    })
    .unwrap();
    assert_eq!(colors(&output(&a))[0], Color(255, 0, 0));
    assert!(b.try_recv().is_err());
    app.ui_command(UiCommand::Visibility {
        instance: second,
        visible: true,
    })
    .unwrap();
    let restored = output(&b);
    assert_eq!(colors(&restored)[0], Color(255, 0, 0));
    // Focus uses component index 1 even though its action is hit-region index 0.
    app.ui_command(UiCommand::Input {
        instance: second,
        revision: restored.revision,
        input: UiInput::Keyboard(KeyboardEvent::named("Enter")),
    })
    .unwrap();
}
#[test]
fn shared_scroll_is_clamped_per_viewport_without_render_order_changing_it() {
    let (app, definition) = setup(20);
    let (first, a) = app
        .create_ui(
            definition,
            PresentationSettings {
                viewport: Size {
                    width: 300.0,
                    height: 100.0,
                },
                ..Default::default()
            },
        )
        .unwrap();
    let (second, b) = app
        .create_ui(
            definition,
            PresentationSettings {
                viewport: Size {
                    width: 300.0,
                    height: 200.0,
                },
                ..Default::default()
            },
        )
        .unwrap();
    let initial = output(&a);
    output(&b);
    app.ui_command(UiCommand::Input {
        instance: first,
        revision: initial.revision,
        input: UiInput::MouseWheel {
            delta: WheelDelta::Pixels { x: 0.0, y: -(50.0) },
            modifiers: Default::default(),
        },
    })
    .unwrap();
    output(&a);
    output(&b);
    app.inspect(move |app| {
        assert_eq!(app.uis().definition(definition)?.state().scroll, 50.0);
        assert_eq!(app.uis().instance(first)?.scroll_offset(), 50.0);
        assert_eq!(app.uis().instance(second)?.scroll_offset(), 50.0);
        Ok(())
    })
    .unwrap();
    app.ui_command(UiCommand::Present {
        instance: second,
        settings: PresentationSettings {
            viewport: Size {
                width: 300.0,
                height: 2000.0,
            },
            ..Default::default()
        },
    })
    .unwrap();
    output(&b);
    app.inspect(move |app| {
        assert_eq!(app.uis().definition(definition)?.state().scroll, 50.0);
        assert_eq!(app.uis().instance(first)?.scroll_offset(), 50.0);
        assert_eq!(app.uis().instance(second)?.scroll_offset(), 0.0);
        Ok(())
    })
    .unwrap();
}
