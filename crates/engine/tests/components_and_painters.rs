//! Registration and painting contracts, including associated types that are not Sync.

use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::app::Application,
    component_registry::registry::ComponentRegistry,
    components::button::{ButtonComponent, ButtonProps},
    expression::{context::ExpressionContext, expression::Expression},
    live_model::{
        component::Component,
        part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
        state::{LiveState, PartState},
    },
    painters::{
        button::ButtonPainter, context::PaintContext, painter::Painter, registry::PainterRegistry,
    },
    ui::{
        definition::UiDefinition,
        display_list::{Color, DrawCommand},
        geometry::{Point, Rect, Size},
        presentation::PresentationSettings,
        renderer,
    },
};
use std::{
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
};

struct A;
struct B;
struct Props {
    value: Cell<u32>,
    fail: bool,
    invalid: bool,
}
struct State {
    serial: u64,
    updates: Cell<u32>,
}
static NEXT_SERIAL: AtomicU64 = AtomicU64::new(1);
impl Default for State {
    fn default() -> Self {
        Self {
            serial: NEXT_SERIAL.fetch_add(1, Ordering::Relaxed),
            updates: Cell::new(0),
        }
    }
}
impl Component for A {
    type Props = Props;
    type State = State;
}
impl Component for B {
    type Props = Props;
    type State = State;
}
struct CustomPainter {
    color: Cell<Color>,
}
impl<C: Component<Props = Props, State = State>> Painter<C> for CustomPainter {
    fn paint(&self, context: &mut PaintContext<'_, C>) -> PixuiResult<()> {
        if context.props.fail {
            return Err(pixui_error!("custom painter failure"));
        }
        if context.props.invalid {
            context.emit(DrawCommand::PopClip);
            return Ok(());
        }
        context.fill_rect(context.bounds(), self.color.get());
        context.with_clip(
            Rect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
            },
            |context| {
                context.text(
                    Point { x: 2.0, y: 3.0 },
                    format!(
                        "{} {} {} {} {}",
                        context.props.value.get(),
                        context.state.updates.get(),
                        context.width,
                        context.height,
                        context.focused
                    ),
                    8.0,
                    Color(255, 255, 255),
                );
                Ok(())
            },
        )
    }
}
fn props(_: &ExpressionContext<'_>, settings: &PresentationSettings) -> PixuiResult<Props> {
    Ok(Props {
        value: Cell::new(settings.locale.parse().unwrap_or(0)),
        fail: settings.locale == "fail",
        invalid: settings.locale == "invalid",
    })
}
fn update(_: &Props, state: &mut State) -> PixuiResult<()> {
    state.updates.set(state.updates.get() + 1);
    Ok(())
}
fn painter(color: Color) -> CustomPainter {
    CustomPainter {
        color: Cell::new(color),
    }
}
fn state(state: &LiveState) -> &State {
    let PartState::Component(state) = state.root_state() else {
        panic!("component");
    };
    state.state.downcast_ref().unwrap()
}

#[test]
fn checked_registrations_distinguish_components_sharing_props_and_state() {
    let mut registry = ComponentRegistry::default();
    assert!(registry.register::<A>("").is_err());
    let a = registry.register::<A>("a").unwrap();
    assert!(registry.register::<A>("another-a").is_err());
    assert!(registry.register::<B>("a").is_err());
    let b = registry.register::<B>("b").unwrap();
    assert_eq!(
        registry.descriptor(a).unwrap().props_type,
        registry.descriptor(b).unwrap().props_type
    );
    assert_ne!(
        registry.descriptor(a).unwrap().component_type,
        registry.descriptor(b).unwrap().component_type
    );
    let foreign = ComponentRegistry::default();
    assert!(foreign.descriptor(a).is_err());
    let mut painters = PainterRegistry::default();
    assert!(
        painters
            .register::<ButtonComponent>(&registry, ButtonPainter)
            .is_err()
    );
    painters
        .register::<A>(&registry, painter(Color(1, 2, 3)))
        .unwrap();
    assert!(
        painters
            .register::<A>(&registry, painter(Color(4, 5, 6)))
            .is_err()
    );
    let mut other = ComponentRegistry::default();
    other.register::<A>("a").unwrap();
    other.register::<B>("b").unwrap();
    assert!(
        painters
            .register::<B>(&other, painter(Color(1, 2, 3)))
            .is_err()
    );
}

#[test]
fn missing_painters_and_foreign_handles_fail_before_empty_loops_are_evaluated() {
    let mut app = Application::default();
    let a = app.register_component::<A>("a").unwrap();
    let template = LivePart::ForLoop(ForLoopPart {
        expression: Expression::field(pixui_reflect::FieldIndex(999)),
        body: Box::new(LivePart::Component(ComponentPart::typed(a, props))),
    });
    let error = app
        .register_ui(UiDefinition::new("missing", template))
        .unwrap_err();
    assert!(format!("{error:?}").contains("missing painter"));
    app.register_painter::<A>(painter(Color(1, 2, 3))).unwrap();
    let mut other = Application::default();
    assert!(
        other
            .register_ui(UiDefinition::new(
                "foreign",
                LivePart::Component(ComponentPart::typed(a, props))
            ))
            .is_err()
    );
    let handle = Application::new();
    let button = handle
        .register_component::<ButtonComponent>("button")
        .unwrap();
    handle
        .register_painter::<ButtonComponent>(ButtonPainter)
        .unwrap();
    handle
        .register_ui(UiDefinition::new(
            "button",
            LivePart::Component(ComponentPart::typed(button, |_, _| {
                Ok(ButtonProps {
                    label: "test".into(),
                })
            })),
        ))
        .unwrap();
}

static RESOLVES: AtomicU64 = AtomicU64::new(0);
fn counted_props(
    context: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<Props> {
    RESOLVES.fetch_add(1, Ordering::Relaxed);
    props(context, settings)
}

#[test]
fn default_state_persists_props_refresh_and_updates_run_once_before_painting() {
    let mut app = Application::default();
    let id = app.register_component::<A>("a").unwrap();
    app.register_painter::<A>(painter(Color(1, 2, 3))).unwrap();
    let root = LivePart::Component(ComponentPart::typed_with_update(id, counted_props, update));
    let mut live = LiveState::new();
    let settings = PresentationSettings {
        locale: "7".into(),
        ..Default::default()
    };
    let (first, geometry, _) =
        renderer::render(&root, &mut live, &app, &settings, 0.0, None, None).unwrap();
    let serial = state(&live).serial;
    assert_eq!(state(&live).updates.get(), 1);
    assert!(first.commands.iter().any(
        |command| matches!(command, DrawCommand::DrawText { text, .. } if text.starts_with("7 1 "))
    ));
    assert_eq!(
        geometry.component_bounds[0].height,
        renderer::COMPONENT_HEIGHT
    );
    let settings = PresentationSettings {
        locale: "9".into(),
        viewport: Size {
            width: 232.0,
            height: 100.0,
        },
        ..Default::default()
    };
    let (second, _, _) =
        renderer::render(&root, &mut live, &app, &settings, 0.0, None, None).unwrap();
    assert_eq!(state(&live).serial, serial);
    assert_eq!(state(&live).updates.get(), 2);
    assert_eq!(RESOLVES.load(Ordering::Relaxed), 2);
    assert!(second.commands.iter().any(|command| matches!(command, DrawCommand::DrawText { text, origin, .. } if text == "9 2 200 36 false" && *origin == Point { x: 18.0, y: 19.0 })));
    assert!(second.commands.iter().any(|command| matches!(command, DrawCommand::PushClip { rect } if *rect == Rect { x: 17.0, y: 18.0, width: 3.0, height: 4.0 })));
}

#[test]
fn replacing_component_type_resets_state_even_when_associated_types_match() {
    let mut app = Application::default();
    let a = app.register_component::<A>("a").unwrap();
    let b = app.register_component::<B>("b").unwrap();
    app.register_painter::<A>(painter(Color(1, 2, 3))).unwrap();
    app.register_painter::<B>(painter(Color(1, 2, 3))).unwrap();
    let mut live = LiveState::new();
    let settings = PresentationSettings::default();
    renderer::render(
        &LivePart::Component(ComponentPart::typed_with_update(a, props, update)),
        &mut live,
        &app,
        &settings,
        0.0,
        None,
        None,
    )
    .unwrap();
    let serial = state(&live).serial;
    renderer::render(
        &LivePart::Component(ComponentPart::typed(b, props)),
        &mut live,
        &app,
        &settings,
        0.0,
        None,
        None,
    )
    .unwrap();
    assert_ne!(state(&live).serial, serial);
    assert_eq!(state(&live).updates.get(), 0);
}

#[test]
fn separate_applications_choose_different_painters_and_zero_width_is_valid() {
    fn draw(color: Color) -> pixui_engine::ui::display_list::DisplayList {
        let mut app = Application::default();
        let id = app.register_component::<A>("a").unwrap();
        app.register_painter::<A>(painter(color)).unwrap();
        let root = LivePart::Component(ComponentPart::typed(id, props));
        renderer::render(
            &root,
            &mut LiveState::new(),
            &app,
            &PresentationSettings::default(),
            0.0,
            None,
            None,
        )
        .unwrap()
        .0
    }
    assert_ne!(draw(Color(10, 20, 30)), draw(Color(30, 20, 10)));
    let mut app = Application::default();
    let id = app.register_component::<A>("a").unwrap();
    app.register_painter::<A>(painter(Color(1, 2, 3))).unwrap();
    let (display, geometry, _) = renderer::render(
        &LivePart::Component(ComponentPart::typed(id, props)),
        &mut LiveState::new(),
        &app,
        &PresentationSettings {
            viewport: Size {
                width: 0.0,
                height: 0.0,
            },
            ..Default::default()
        },
        0.0,
        None,
        None,
    )
    .unwrap();
    display.validate().unwrap();
    assert_eq!(geometry.component_bounds[0].width, 0.0);
}

#[test]
fn fixed_rows_translate_in_order_and_scrolling_is_clamped() {
    let mut app = Application::default();
    let id = app.register_component::<A>("a").unwrap();
    app.register_painter::<A>(painter(Color(1, 2, 3))).unwrap();
    let root = LivePart::Composite(CompositePart {
        parts: vec![
            LivePart::Component(ComponentPart::typed(id, props)),
            LivePart::Component(ComponentPart::typed(id, props)),
        ],
    });
    let (display, geometry, _) = renderer::render(
        &root,
        &mut LiveState::new(),
        &app,
        &PresentationSettings::default(),
        0.0,
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        geometry.component_bounds[1].y - geometry.component_bounds[0].y,
        44.0
    );
    let origins: Vec<_> = display
        .commands
        .iter()
        .filter_map(|command| {
            if let DrawCommand::DrawText { origin, .. } = command {
                Some(origin.y)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(origins, [19.0, 63.0]);
    let (_, geometry, scroll) = renderer::render(
        &root,
        &mut LiveState::new(),
        &app,
        &PresentationSettings {
            viewport: Size {
                width: 200.0,
                height: 40.0,
            },
            ..Default::default()
        },
        1000.0,
        None,
        None,
    )
    .unwrap();
    assert_eq!(scroll, geometry.content_height - 40.0);
}

#[test]
fn painter_failures_and_invalid_commands_retain_last_good_output() {
    let app = Application::new();
    let id = app.register_component::<A>("a").unwrap();
    app.register_painter::<A>(painter(Color(1, 2, 3))).unwrap();
    let definition = app
        .register_ui(UiDefinition::new(
            "test",
            LivePart::Component(ComponentPart::typed(id, props)),
        ))
        .unwrap();
    let (instance, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let good = outputs
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    for locale in ["fail", "invalid"] {
        app.ui_command(pixui_engine::ui::input::UiCommand::Present {
            instance,
            settings: PresentationSettings {
                locale: locale.into(),
                ..Default::default()
            },
        })
        .unwrap();
        let (revision, error) = app
            .inspect(move |app| {
                Ok((
                    app.uis().instance(instance)?.revision(),
                    app.uis()
                        .instance(instance)?
                        .last_error()
                        .map(str::to_owned),
                ))
            })
            .unwrap();
        assert_eq!(revision, good.revision);
        assert!(error.is_some());
        assert_eq!(
            outputs.try_recv(),
            Err(crossbeam_channel::TryRecvError::Empty)
        );
    }
    app.ui_command(pixui_engine::ui::input::UiCommand::Present {
        instance,
        settings: PresentationSettings::default(),
    })
    .unwrap();
    let recovered = outputs
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    assert!(recovered.revision > good.revision);
}

#[test]
fn failed_updates_stop_rendering_and_non_sync_types_are_supported() {
    let mut app = Application::default();
    let id = app.register_component::<A>("a").unwrap();
    app.register_painter::<A>(painter(Color(1, 2, 3))).unwrap();
    // A typed update itself can fail; it must stop the painter and output publication.
    let broken = LivePart::Component(ComponentPart::typed_with_update(id, props, |_, _| {
        Err(pixui_error!("update failure"))
    }));
    assert!(
        renderer::render(
            &broken,
            &mut LiveState::new(),
            &app,
            &PresentationSettings::default(),
            0.0,
            None,
            None
        )
        .is_err()
    );
    fn assert_send<T: Send + 'static>() {}
    assert_send::<Props>();
    assert_send::<State>();
    assert_send::<CustomPainter>();
    assert_send::<ComponentPart>();
}
