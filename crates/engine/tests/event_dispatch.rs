//! Custom component events through the real worker, source compiler and action dispatcher.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::{
        app::Application, application_handle::ApplicationHandle,
        application_slice::ApplicationSlice,
    },
    events::interface::{ComponentInput, InputInterest},
    live_model::component::Component,
    painters::{context::PaintContext, measure::MeasureContext, painter::Painter},
    ui::{
        definition::UiDefinitionId,
        display_list::{Color, RenderOutput},
        geometry::{Point, Size},
        input::{ButtonState, KeyboardEvent, MouseButton, UiCommand, UiInput},
        instance::UiInstanceId,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
    },
    ui_language::{compiler::compile, resolution::environment::Environment},
};
use pixui_engine_macros::component_events;
use std::time::Duration;
#[component_events]
trait ProbeEvents {
    fn record(&self, value: String) -> PixuiResult<()>;
}
#[pixui_reflect::reflect(send)]
mod data {
    pub struct Props {
        pub mode: String,
    }
}
struct Probe;
impl Component for Probe {
    type Props = data::Props;
    type State = u32;
    type Events = dyn ProbeEvents;
    const INPUT_INTEREST: InputInterest = InputInterest {
        keyboard: true,
        pointer: true,
        click: true,
        ..InputInterest::NONE
    };
    fn handle_input(
        props: &Self::Props,
        state: &mut Self::State,
        input: &ComponentInput,
        events: &Self::Events,
    ) -> PixuiResult<bool> {
        match input {
            ComponentInput::Raw(UiInput::PointerMoved(_)) if props.mode == "state-only" => {
                *state += 1;
                Ok(false)
            }
            ComponentInput::Raw(UiInput::PointerMoved(_)) if props.mode == "pointer" => {
                events.record("moved".into())?;
                Ok(true)
            }
            ComponentInput::Click {
                button: MouseButton::Left,
                ..
            } => {
                *state += 1;
                events.record(format!("{}:{state}", props.mode))?;
                if props.mode == "ordered" {
                    events.record("second".into())?;
                    events.record("third".into())?;
                }
                if props.mode == "build-failure" {
                    events
                        .record("x".repeat(pixui_engine::i18n::template::MAX_OUTPUT_BYTES + 1))?;
                    events.record("must-not-run".into())?;
                }
                if props.mode == "failure" {
                    events.record("fail".into())?;
                    events.record("must-not-run".into())?;
                }
                Ok(true)
            }
            ComponentInput::Raw(UiInput::Keyboard(event)) if matches!(&event.key, pixui_engine::ui::input::Key::Named(name) if name == "Enter") =>
            {
                events.record("keyboard".into())?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
struct ProbePainter;
impl Painter<Probe> for ProbePainter {
    fn measure(&self, context: &MeasureContext<'_, Probe>) -> PixuiResult<Size> {
        Ok(context.constrain(Size {
            width: 100.0,
            height: 40.0,
        }))
    }
    fn paint(&self, context: &mut PaintContext<'_, Probe>) -> PixuiResult<()> {
        context.fill_rect(context.bounds(), Color(*context.state as u8, 0, 0));
        Ok(())
    }
}
#[pixui_engine::application::action::slice_actions(slice="events", facade=Actions)]
mod actions {
    #[action]
    pub fn record(
        mut log: pixui_engine::application::entity_mut::EntityMut<Vec<String>>,
        text: String,
    ) -> pixui_base::PixuiResult<()> {
        log.push(text.clone());
        if text == "fail" {
            return Err(pixui_base::pixui_error!("requested failure"));
        }
        Ok(())
    }
}
struct Fixture {
    app: ApplicationHandle,
    definition: UiDefinitionId,
    instance: UiInstanceId,
    outputs: OutputReceiver,
    output: RenderOutput,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let app = Application::new();
        let mut slice = ApplicationSlice::new("events");
        slice.bind("log", Vec::<String>::new()).unwrap();
        let slice = app.add_slice(slice).unwrap();
        actions::Actions::register(&app, slice).unwrap();
        let component = app.register_component::<Probe>("probe").unwrap();
        app.register_painter::<Probe>(ProbePainter).unwrap();
        let source = format!(
            "ui Events {{<Column slice='events'><Probe mode='{mode}' focus='sequential' @record=record(text: '{{value}}') /></Column>}}"
        );
        let definition = app
            .inspect(move |app| {
                let mut env = Environment::new();
                env.component("Probe", component)?;
                env.action(
                    app,
                    "events",
                    "record",
                    app.slice(slice)?.action_handle_named("record")?,
                )?;
                compile("worker-events.pixui", &source, &env)
                    .map(|compiled| compiled.definition)
                    .map_err(|error| pixui_error!("{error}"))
            })
            .unwrap();
        let definition = app.register_ui(definition).unwrap();
        let (instance, outputs) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let output = outputs.recv_timeout(Duration::from_secs(3)).unwrap();
        Self {
            app,
            definition,
            instance,
            outputs,
            output,
        }
    }
    fn log(&self) -> Vec<String> {
        self.app
            .inspect(|app| {
                Ok(app
                    .entity::<Vec<String>>(app.slice_named("events")?.id(), "log")?
                    .clone())
            })
            .unwrap()
    }
    fn mouse(&self, state: ButtonState, position: Point) -> PixuiResult<()> {
        self.app.ui_command(UiCommand::Input {
            instance: self.instance,
            revision: self.output.revision,
            input: UiInput::MouseButton {
                button: MouseButton::Left,
                state,
                position,
                modifiers: Default::default(),
            },
        })
    }
    fn point(&self) -> Point {
        let id = self.instance;
        self.app
            .inspect(move |app| {
                let rect = app.uis().instance(id)?.layout().component_bounds[0];
                Ok(Point {
                    x: rect.x + 4.0,
                    y: rect.y + 4.0,
                })
            })
            .unwrap()
    }
    fn click(&self) -> PixuiResult<()> {
        let point = self.point();
        self.mouse(ButtonState::Pressed, point)?;
        self.mouse(ButtonState::Released, point)
    }
    fn refresh(&mut self) {
        self.app.inspect(|_| Ok(())).unwrap();
        self.output = self.outputs.recv_timeout(Duration::from_secs(3)).unwrap();
    }
}
#[test]
fn ordered_emissions_dispatch_once_and_retain_component_state() {
    let mut fixture = Fixture::new("ordered");
    fixture.click().unwrap();
    assert_eq!(fixture.log(), ["ordered:1", "second", "third"]);
    fixture.refresh();
    fixture.click().unwrap();
    assert_eq!(
        fixture.log(),
        [
            "ordered:1",
            "second",
            "third",
            "ordered:2",
            "second",
            "third"
        ]
    );
}
#[test]
fn errors_stop_remaining_actions_and_preserve_prior_mutations() {
    let fixture = Fixture::new("failure");
    let error = fixture.click().unwrap_err();
    let report = format!("{error:?}");
    assert!(report.contains("requested failure"));
    assert!(report.contains("worker-events.pixui"));
    assert!(report.contains("record"));
    assert_eq!(fixture.log(), ["failure:1", "fail"]);
}
#[test]
fn pointer_gestures_require_a_matching_target_and_visibility_cancels_press() {
    let fixture = Fixture::new("one");
    let inside = fixture.point();
    fixture.mouse(ButtonState::Released, inside).unwrap();
    assert!(fixture.log().is_empty());
    fixture.mouse(ButtonState::Pressed, inside).unwrap();
    fixture
        .mouse(ButtonState::Released, Point { x: 300.0, y: 300.0 })
        .unwrap();
    assert!(fixture.log().is_empty());
    fixture.mouse(ButtonState::Pressed, inside).unwrap();
    fixture
        .app
        .ui_command(UiCommand::Visibility {
            instance: fixture.instance,
            visible: false,
        })
        .unwrap();
    fixture
        .app
        .ui_command(UiCommand::Visibility {
            instance: fixture.instance,
            visible: true,
        })
        .unwrap();
    fixture.mouse(ButtonState::Released, inside).unwrap();
    assert!(fixture.log().is_empty());
}
#[test]
fn custom_keyboard_consumption_and_two_source_windows_do_not_duplicate_events() {
    let mut fixture = Fixture::new("one");
    let (peer, output) = fixture
        .app
        .create_ui(
            fixture.definition,
            PresentationSettings {
                viewport: Size {
                    width: 200.0,
                    height: 300.0,
                },
                ..Default::default()
            },
        )
        .unwrap();
    output.recv_timeout(Duration::from_secs(3)).unwrap();
    fixture.click().unwrap();
    fixture.refresh();
    fixture
        .app
        .ui_command(UiCommand::Input {
            instance: fixture.instance,
            revision: fixture.output.revision,
            input: UiInput::Keyboard(KeyboardEvent::named("Enter")),
        })
        .unwrap();
    assert_eq!(fixture.log(), ["one:1", "keyboard"]);
    assert_ne!(fixture.instance, peer);
}

#[test]
fn request_building_failure_preserves_earlier_emissions_and_stops_the_batch() {
    let fixture = Fixture::new("build-failure");
    let error = fixture.click().unwrap_err();
    assert!(format!("{error:?}").contains("worker-events.pixui"));
    assert_eq!(fixture.log(), ["build-failure:1"]);
}

fn rust_record(
    context: &pixui_engine::expression::context::ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<pixui_engine::events::binding::TypedBinding<ProbeEventsRecordPayload>> {
    let handle = context
        .application()?
        .slice_named("events")?
        .action_handle_named("record")?;
    Ok(Box::new(move |_, payload| {
        handle.call(vec![Box::new(payload.value.clone())])
    }))
}
#[test]
fn generated_rust_keys_bind_typed_callbacks_and_duplicates_fail_registration() {
    let fixture = Fixture::new("one");
    let component = fixture
        .app
        .inspect(|app| app.components().component::<Probe>())
        .unwrap();
    let part = pixui_engine::live_model::part::ComponentPart::typed(component, |_, _| {
        Ok(data::Props {
            mode: "rust".into(),
        })
    })
    .with_layout(pixui_engine::layout::style::LayoutStyle::fixed(100.0, 40.0))
    .with_event(ProbeEventsBindings::RECORD, rust_record);
    assert!(
        fixture
            .app
            .register_ui(pixui_engine::ui::definition::UiDefinition::new(
                "duplicate",
                part.clone()
                    .with_event(ProbeEventsBindings::RECORD, rust_record)
                    .into()
            ))
            .is_err()
    );
    let definition = fixture
        .app
        .register_ui(pixui_engine::ui::definition::UiDefinition::new(
            "rust",
            part.into(),
        ))
        .unwrap();
    let (id, outputs) = fixture
        .app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let output = outputs.recv_timeout(Duration::from_secs(3)).unwrap();
    let point = fixture
        .app
        .inspect(move |app| {
            let rect = app.uis().instance(id)?.layout().component_bounds[0];
            Ok(Point {
                x: rect.x + 4.0,
                y: rect.y + 4.0,
            })
        })
        .unwrap();
    for state in [ButtonState::Pressed, ButtonState::Released] {
        fixture
            .app
            .ui_command(UiCommand::Input {
                instance: id,
                revision: output.revision,
                input: UiInput::MouseButton {
                    button: MouseButton::Left,
                    state,
                    position: point,
                    modifiers: Default::default(),
                },
            })
            .unwrap();
    }
    assert_eq!(fixture.log(), ["rust:1"]);
}

#[test]
fn consumed_pointer_input_still_updates_shared_hover_and_leaving_clears_it() {
    let fixture = Fixture::new("pointer");
    fixture
        .app
        .ui_command(UiCommand::Input {
            instance: fixture.instance,
            revision: fixture.output.revision,
            input: UiInput::PointerMoved(fixture.point()),
        })
        .unwrap();
    assert_eq!(fixture.log(), ["moved"]);
    let definition = fixture.definition;
    assert!(
        fixture
            .app
            .inspect(move |app| Ok(app.uis().definition(definition)?.state().hover.is_some()))
            .unwrap()
    );
    fixture
        .app
        .ui_command(UiCommand::Input {
            instance: fixture.instance,
            revision: fixture.output.revision,
            input: UiInput::PointerLeft,
        })
        .unwrap();
    assert!(
        fixture
            .app
            .inspect(move |app| Ok(app.uis().definition(definition)?.state().hover.is_none()))
            .unwrap()
    );
}

#[test]
fn local_state_changes_repaint_without_emitting_actions_or_consuming_input() {
    let mut fixture = Fixture::new("state-only");
    for expected in [1, 2] {
        fixture
            .app
            .ui_command(UiCommand::Input {
                instance: fixture.instance,
                revision: fixture.output.revision,
                input: UiInput::PointerMoved(fixture.point()),
            })
            .unwrap();
        fixture.refresh();
        assert!(fixture.output.display_list.commands.iter().any(|command| matches!(command,
            pixui_engine::ui::display_list::DrawCommand::FillRect { color: Color(red, 0, 0), .. } if *red == expected
        )));
    }
    assert!(fixture.log().is_empty());
}
