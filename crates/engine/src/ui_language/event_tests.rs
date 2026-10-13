//! Callback scopes, typed payloads and named requests through the actual compiler.
use super::{compiler::compile, resolution::environment::Environment};
use crate::{
    application::{
        action::ActionDescriptor, app::Application, application_slice::ApplicationSlice,
    },
    components::text_input::TextInputComponent,
    events::{
        binding::{Callback, EventBinding},
        interface::{EventInterface, EventSink},
        standard::TextInputEvents,
    },
    expression::context::ExpressionContext,
    live_model::{
        part::LivePart,
        state::LiveState,
        walk::{Visitor, WalkEntry, walk},
    },
};
use pixui_base::PixuiResult;

#[pixui_reflect::reflect(send)]
mod data {
    pub struct Request {
        pub text: String,
        pub accepted: bool,
    }
    #[derive(Clone)]
    pub struct Row {
        pub content: bool,
        pub label: String,
    }
}
fn descriptor() -> &'static ActionDescriptor {
    static DESCRIPTOR: std::sync::OnceLock<ActionDescriptor> = std::sync::OnceLock::new();
    DESCRIPTOR.get_or_init(|| {
        ActionDescriptor::new::<data::Request>(
            "edit",
            "test callback",
            vec![],
            |app, slice, request| {
                let request = request.downcast::<data::Request>().unwrap();
                *app.entity_mut::<String>(slice, "result")? =
                    format!("{}:{}", request.text, request.accepted);
                Ok(Box::new(()))
            },
        )
    })
}
fn fixture() -> (Application, Environment) {
    let mut app = Application::default();
    let input = app
        .register_component::<TextInputComponent>("input")
        .unwrap();
    let mut env = Environment::new();
    env.component("TextInput", input).unwrap();
    for name in ["one", "two"] {
        let mut slice = ApplicationSlice::new(name);
        slice.bind("result", String::new()).unwrap();
        let slice = app.add_slice(slice).unwrap();
        let action = app.register_action(slice, descriptor()).unwrap();
        let handle = app.slice(slice).unwrap().action_handle(action).unwrap();
        env.action(&app, name, "edit", handle).unwrap();
    }
    let one = app.slice_named("one").unwrap().id();
    let reference = app.entity_ref::<String>(one, "result").unwrap();
    env.entity("result", &app, reference).unwrap();
    (app, env)
}
fn bindings(app: &Application, source: &str, env: &Environment) -> Vec<EventBinding> {
    let mut definition = compile("events.pixui", source, env).unwrap().definition;
    crate::i18n::registration::register(&mut definition, &mut app.translations.clone()).unwrap();
    struct Collect(Vec<EventBinding>);
    impl Visitor for Collect {
        fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
            if let LivePart::Component(part) = &*entry.part {
                for event in &part.events {
                    if let Callback::Program(program) = &event.callback {
                        self.0.push(program.prepare(entry.context)?);
                    }
                }
            }
            Ok(())
        }
    }
    let mut collect = Collect(vec![]);
    walk(
        &mut definition.template,
        LiveState::new().root_state_mut(),
        &ExpressionContext::new(app),
        &mut collect,
    )
    .unwrap();
    collect.0
}
fn payload(content: &str) -> pixui_reflect::DynamicObject<'static> {
    let sink = EventSink::new(<dyn TextInputEvents>::descriptors(), vec![true, false]).unwrap();
    <dyn TextInputEvents>::with_proxy(&sink, |events| events.change(content.into()).unwrap());
    sink.into_pending().remove(0).payload
}
#[test]
fn container_scopes_override_lexically_and_named_arguments_reorder_fields() {
    let (mut app, env) = fixture();
    let callbacks = bindings(
        &app,
        r#"ui X {<Column slice='one'>
        <TextInput content='' @change=edit(accepted: true, text: content) />
        <Column slice='two'><TextInput content='' @change=edit(text: event.content, accepted: false) /></Column>
        <TextInput content='' @change=edit(text: content, accepted: true) />
        <TextInput content='' @change=two.edit(text: content, accepted: true) />
    </Column>}"#,
        &env,
    );
    for (index, callback) in callbacks.iter().enumerate() {
        app.dispatch(callback(&app, &payload(&format!("{index}"))).unwrap())
            .unwrap();
    }
    assert_eq!(
        app.entity::<String>(app.slice_named("one").unwrap().id(), "result")
            .unwrap(),
        "2:true"
    );
    assert_eq!(
        app.entity::<String>(app.slice_named("two").unwrap().id(), "result")
            .unwrap(),
        "3:true"
    );
}
#[test]
fn event_values_are_fresh_but_application_values_remain_published_snapshots() {
    let (mut app, env) = fixture();
    let slice = app.slice_named("one").unwrap().id();
    *app.entity_mut::<String>(slice, "result").unwrap() = "old".into();
    let callbacks = bindings(
        &app,
        r#"ui X {<Column slice='one'>
        <TextInput content='' @change=edit(text: '{entities.result}:{content}', accepted: true) />
    </Column>}"#,
        &env,
    );
    *app.entity_mut::<String>(slice, "result").unwrap() = "new".into();
    for text in ["first", "second"] {
        app.dispatch(callbacks[0](&app, &payload(text)).unwrap())
            .unwrap();
        assert_eq!(
            app.entity::<String>(slice, "result").unwrap(),
            &format!("old:{text}:true")
        );
    }
}
#[test]
fn event_parameters_shadow_loop_names_and_fragments_preserve_slice_scope() {
    let (mut app, mut env) = fixture();
    let slice = app.slice_named("one").unwrap().id();
    let reference = app
        .create_entity(vec![data::Row {
            content: false,
            label: "loop".into(),
        }])
        .unwrap();
    env.entity("rows", &app, reference).unwrap();
    env.value_type::<data::Row>();
    let callbacks = bindings(
        &app,
        r#"ui X {<Column slice='one'>
        {for content in entities.rows => <>
            {match true; true => <TextInput content='' @change=edit(text: content, accepted: true) />, false => <></>,}
        </>}
    </Column>}"#,
        &env,
    );
    app.dispatch(callbacks[0](&app, &payload("event")).unwrap())
        .unwrap();
    assert_eq!(app.entity::<String>(slice, "result").unwrap(), "event:true");
}
#[test]
fn invalid_callbacks_and_named_calls_report_source_spans() {
    let (_app, env) = fixture();
    for (source, message) in [
        (
            "ui X {<TextInput content='' @change=edit(text: content, accepted: true) />}",
            "enclosing container",
        ),
        (
            "ui X {<TextInput content='' @change=missing.edit(text: content, accepted: true) />}",
            "unknown exported slice",
        ),
        (
            "ui X {<TextInput content='' @click=one.edit(text: content, accepted: true) />}",
            "unknown event",
        ),
        (
            "ui X {<TextInput content='' @change=one.edit(text: content) />}",
            "missing action argument",
        ),
        (
            "ui X {<TextInput content='' @change=one.edit(text: content, accepted: true, text: '') />}",
            "duplicate action argument",
        ),
        (
            "ui X {<TextInput content='' @change=one.edit(text: content, accepted: true, other: true) />}",
            "unknown action argument",
        ),
        (
            "ui X {<TextInput content='' @change=one.edit(text: event.missing, accepted: true) />}",
            "unknown field",
        ),
        (
            "ui X {<TextInput content='' @change=one.edit(text: content, accepted: content) />}",
            "incompatible expression type",
        ),
        ("ui X {<TextInput content=content />}", "unknown name"),
        (
            "ui X {<TextInput content='' @submit=one.edit(text: content, accepted: true) />}",
            "unknown name",
        ),
        (
            "ui X {<TextInput content='' @change=one.edit(content, true) />}",
            "Colon",
        ),
        (
            "ui X {<TextInput content='' @change=(value) => one.edit(text: value, accepted: true) />}",
            "expected identifier",
        ),
    ] {
        let errors = match compile("bad-events.pixui", source, &env) {
            Ok(_) => panic!("accepted {source}"),
            Err(errors) => errors,
        };
        assert!(errors.to_string().contains(message), "{source}: {errors}");
        for error in errors.diagnostics {
            assert!(error.primary.end <= source.len());
        }
    }
}
#[test]
fn action_exports_check_slice_ownership_and_do_not_export_peer_actions() {
    let (app, mut env) = fixture();
    let one = app.slice_named("one").unwrap();
    let handle = one.action_handle_named("edit").unwrap();
    assert!(env.action(&app, "two", "other", handle).is_err());
    assert!(env.action(&app, "one", "edit", handle).is_err());
    assert!(
        compile(
            "scope.pixui",
            "ui X {<Column slice='one'><TextInput content='' @change=missing() /></Column>}",
            &env
        )
        .is_err()
    );
}

#[test]
fn loop_field_chains_are_owned_snapshots_and_payload_conditions_remain_lazy() {
    let (mut app, mut env) = fixture();
    let reference = app
        .create_entity(vec![data::Row {
            content: true,
            label: "old".into(),
        }])
        .unwrap();
    env.entity("rows", &app, reference).unwrap();
    env.value_type::<data::Row>();
    let callbacks = bindings(
        &app,
        r#"ui X {<Column slice='one'>
        {for row in entities.rows => <TextInput content='' @change=edit(
            text: '{row.label}:{content}', accepted: content == '' || row.content,
        ) />}
    </Column>}"#,
        &env,
    );
    app.resolve_mut(reference).unwrap()[0].label = "new".into();
    app.resolve_mut(reference).unwrap()[0].content = false;
    app.dispatch(callbacks[0](&app, &payload("hello")).unwrap())
        .unwrap();
    assert_eq!(
        app.entity::<String>(app.slice_named("one").unwrap().id(), "result")
            .unwrap(),
        "old:hello:true"
    );
}

#[pixui_reflect::reflect]
mod detail {
    pub struct NonClone {
        pub value: String,
        pub marker: std::cell::Cell<bool>,
    }
}
#[pixui_engine_macros::component_events(crate)]
trait DetailEvents {
    fn change(&self, detail: detail::NonClone) -> PixuiResult<()>;
}
struct Details;
impl crate::live_model::component::Component for Details {
    type Props = crate::components::text_input::TextInputProps;
    type State = ();
    type Events = dyn DetailEvents;
}
#[test]
fn custom_event_parameters_can_be_inspected_without_clone_or_sync_bounds() {
    let (mut app, mut env) = fixture();
    let component = app.register_component::<Details>("details").unwrap();
    env.component("Details", component).unwrap();
    let callbacks = bindings(
        &app,
        "ui X {<Details content='' @change=one.edit(text: detail.value, accepted: true) />}",
        &env,
    );
    let sink = EventSink::new(<dyn DetailEvents>::descriptors(), vec![true]).unwrap();
    <dyn DetailEvents>::with_proxy(&sink, |events| {
        events
            .change(detail::NonClone {
                value: "owned".into(),
                marker: std::cell::Cell::new(false),
            })
            .unwrap()
    });
    let payload = sink.into_pending().remove(0).payload;
    app.dispatch(callbacks[0](&app, &payload).unwrap()).unwrap();
    assert_eq!(
        app.entity::<String>(app.slice_named("one").unwrap().id(), "result")
            .unwrap(),
        "owned:true"
    );
}
