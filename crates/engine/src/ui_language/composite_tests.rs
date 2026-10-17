//! Composite props, child projection and declaration checking through the compiler.
use super::{
    compiler::{compile, compile_module},
    resolution::environment::Environment,
};
use crate::{
    application::app::Application,
    expression::{context::ExpressionContext, evaluator::evaluate},
    live_model::{
        part::LivePart,
        state::LiveState,
        walk::{Visitor, WalkEntry, walk},
    },
};
use pixui_base::PixuiResult;
fn fixture() -> (Application, Environment) {
    let mut app = Application::default();
    let components = crate::painters::standard::register_components(&mut app.components).unwrap();
    crate::painters::standard::register_painters(
        &mut app.painters,
        &app.components,
        &mut app.styles,
    )
    .unwrap();
    let mut env = Environment::new();
    env.component("Label", components.label).unwrap();
    env.component("Button", components.button).unwrap();
    env.component("Checkbox", components.checkbox).unwrap();
    env.component("Image", components.image).unwrap();
    (app, env)
}
struct Values(Vec<String>);
impl Visitor for Values {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
        if let LivePart::Component(part) = &*entry.part {
            for expression in &part.expressions {
                if let Some(text) = evaluate(entry.context, expression)?.downcast_ref::<String>() {
                    self.0.push(text.clone());
                }
            }
        }
        Ok(())
    }
}
fn texts(app: &mut Application, env: &Environment, source: &str) -> Vec<String> {
    let mut definition = compile("composites.pixui", source, env).unwrap().definition;
    crate::i18n::registration::register(&mut definition, &mut app.translations).unwrap();
    let mut values = Values(vec![]);
    walk(
        &mut definition.template,
        LiveState::default().root_state_mut(),
        &ExpressionContext::new(app),
        &mut values,
    )
    .unwrap();
    values.0
}
#[test]
fn local_forward_components_and_nested_slots_keep_caller_props() {
    let (mut app, env) = fixture();
    let source = r#"
        component Metric(title: String, value: String) {
            <Card><Label text=props.title /><Label text=props.value /></Card>
        }
        component Card(children: Children) { <Column padding=8 gap=4><Children /></Column> }
        ui X {<>
            <Metric value='12' title="Projects" />
            <Card><Card><Label text='nested' /></Card></Card>
            <Card />
        </>}
    "#;
    assert_eq!(texts(&mut app, &env, source), ["Projects", "12", "nested"]);
}
#[test]
fn scalar_props_and_loop_slots_resolve_in_the_correct_frames() {
    let (mut app, mut env) = fixture();
    let rows = app
        .create_entity(vec!["one".to_owned(), "two".to_owned()])
        .unwrap();
    env.entity("rows", &app, rows).unwrap();
    let source = r#"
        component Card(children: Children) { <Column><Children /></Column> }
        component Badge(title: String, count: u64, checked: bool) {
            <Card><Label text='{props.title}:{props.count}' /><Checkbox label=props.title checked=props.checked /></Card>
        }
        ui X {<>
            <Badge checked=true title='badge' count=7 />
            {for row in entities.rows => <Card><Label text=row /></Card>}
        </>}
    "#;
    assert_eq!(
        texts(&mut app, &env, source),
        ["badge:7", "badge", "one", "two"]
    );
}
#[test]
fn library_modules_export_only_explicit_public_components() {
    let (_, env) = fixture();
    let compiled = compile_module("library.pixui", "component Private() {<></>} export component Public(title: String) {<Label text=props.title />}", &env).unwrap();
    assert!(compiled.entry.is_none());
    assert_eq!(compiled.exports.len(), 1);
    assert!(compiled.exports.contains_key("Public"));
    assert!(!compiled.exports.contains_key("Private"));
    assert!(compiled.exports["Public"].reference().is_err());
}
#[test]
fn invalid_props_slots_cycles_and_legacy_imports_are_diagnosed() {
    let (_, env) = fixture();
    for (source, expected) in [
        (
            "component X(title: String) {<Label text=props.title />} ui Main {<X />}",
            "missing prop",
        ),
        (
            "component X(title: String) {<Label text=props.title />} ui Main {<X title=true />}",
            "incompatible",
        ),
        (
            "component X() {<></>} ui Main {<X unknown=1 />}",
            "unknown prop",
        ),
        (
            "component X(title: String) {<></>} ui Main {<X title='' title='' />}",
            "duplicate",
        ),
        (
            "component X() {<Children />} ui Main {<X />}",
            "declared children",
        ),
        (
            "component X() {<></>} ui Main {<X><Label text='' /></X>}",
            "child slot",
        ),
        (
            "component X(children: Children) {<Children x=1 />} ui Main {<X />}",
            "no attributes",
        ),
        (
            "component X(children: Children) {<><Children /><Children /></>} ui Main {<X />}",
            "at most one",
        ),
        (
            "component X(title: Unknown) {<></>} ui Main {<X title='' />}",
            "unknown prop type",
        ),
        (
            "component X(title: String, title: String) {<></>} ui Main {<></>}",
            "duplicate component parameter",
        ),
        (
            "component X() {<Y />} component Y() {<X />} ui Main {<X />}",
            "cyclic",
        ),
        (
            "component X() {<></>} component X() {<></>} ui Main {<></>}",
            "duplicate component",
        ),
        ("component Row() {<></>} ui Main {<></>}", "conflicting"),
        (
            "component X() {<Label text=row />} ui Main {<></>}",
            "unknown name",
        ),
        (
            "ui Main {<Label text=props.title />}",
            "unknown composite prop",
        ),
        (
            "import X from './x.pixui'; ui Main {<X />}",
            "replaced by use",
        ),
    ] {
        let error = compile("invalid-composite.pixui", source, &env)
            .err()
            .unwrap_or_else(|| panic!("accepted {source}"));
        assert!(error.to_string().contains(expected), "{source}: {error}");
        assert!(error.to_string().contains("invalid-composite.pixui"));
    }
}

#[test]
fn named_library_imports_export_signatures_aliases_and_zero_argument_refs() {
    use super::{
        bundle::{BundleLimits, install},
        source::bundle::SourceBundle,
    };
    let (mut app, env) = fixture();
    let mut bundle = SourceBundle::new();
    bundle.add("common.pixui", "component Private() {<></>} export component Card(children: Children) {<Column><Children /></Column>} export component Title(text: String) {<Label text=props.text />} export component Empty() {<></>}").unwrap();
    bundle.add("root.pixui", "use { Card as Panel, Title } from './common.pixui'; ui X domain 'test' {<Panel><Title text=\"Named\" /></Panel>}").unwrap();
    let installed = install(
        &mut app,
        &bundle,
        &["root.pixui".into()],
        &env,
        "test",
        BundleLimits::default(),
    )
    .unwrap();
    assert!(
        installed
            .component("common.pixui")
            .unwrap_err()
            .to_string()
            .contains("no ui entry")
    );
    assert!(installed.export("common.pixui", "Private").is_err());
    assert_eq!(
        app.translations
            .declarations("test")
            .iter()
            .filter(|message| message.key.source == "Named")
            .count(),
        1
    );
    let mut part = LivePart::Dynamic(crate::live_model::part::DynamicPart::new(
        crate::expression::expression::Expression::component(
            installed.component("root.pixui").unwrap(),
        ),
    ));
    let mut values = Values(vec![]);
    walk(
        &mut part,
        LiveState::default().root_state_mut(),
        &ExpressionContext::new(&app),
        &mut values,
    )
    .unwrap();
    assert_eq!(values.0, ["Named"]);
    for name in ["Card", "Title"] {
        let reference = installed
            .export("common.pixui", name)
            .unwrap()
            .reference()
            .unwrap();
        let mut part = LivePart::Dynamic(crate::live_model::part::DynamicPart::new(
            crate::expression::expression::Expression::component(reference),
        ));
        assert!(
            walk(
                &mut part,
                LiveState::default().root_state_mut(),
                &ExpressionContext::new(&app),
                &mut Values(vec![])
            )
            .unwrap_err()
            .to_string()
            .contains("requires a static invocation")
        );
    }
    let export = installed.export("common.pixui", "Empty").unwrap();
    let mut host = env.clone();
    host.composite("Empty", &export).unwrap();
    assert!(compile("host.pixui", "ui X {<Empty />}", &host).is_ok());
}
#[test]
fn named_import_errors_include_both_call_and_declaration_sources_and_roll_back() {
    use super::{
        bundle::{BundleLimits, install},
        source::bundle::SourceBundle,
    };
    let (mut app, env) = fixture();
    for (root, expected) in [
        (
            "use { Title } from './common.pixui'; ui X {<Title />}",
            "missing prop",
        ),
        (
            "use { Title } from './common.pixui'; ui X {<Title text=true />}",
            "incompatible",
        ),
        (
            "use { Private } from './common.pixui'; ui X {<Private />}",
            "private export",
        ),
        (
            "use { Title as Label } from './common.pixui'; ui X {<Label />}",
            "conflicting",
        ),
        (
            "use { Title, Title } from './common.pixui'; ui X {<Title />}",
            "duplicate use alias",
        ),
    ] {
        let mut bundle = SourceBundle::new();
        bundle.add("common.pixui", "component Private() {<></>} export component Title(text: String) {<Label text=props.text />}").unwrap();
        bundle.add("root.pixui", root).unwrap();
        let error = install(
            &mut app,
            &bundle,
            &["root.pixui".into()],
            &env,
            "test",
            BundleLimits::default(),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("root.pixui"), "{error}");
        if expected == "missing prop" || expected == "incompatible" {
            assert!(error.contains("common.pixui"), "{error}");
        }
        assert_eq!(app.template_count(), 0);
        assert!(app.translations.declarations("test").is_empty());
    }
}

#[pixui_reflect::reflect(send)]
mod event_data {
    pub struct Record {
        pub text: String,
    }
}
#[test]
fn projected_actions_keep_caller_slice_and_deferred_callbacks_capture_props() {
    use crate::{
        application::{action::ActionDescriptor, application_slice::ApplicationSlice},
        events::binding::{Callback, EventBinding},
    };
    let (mut app, mut env) = fixture();
    static RECORD: std::sync::OnceLock<ActionDescriptor> = std::sync::OnceLock::new();
    let descriptor = RECORD.get_or_init(|| {
        ActionDescriptor::new::<event_data::Record>(
            "record",
            "record text",
            vec![],
            |app, slice, request| {
                *app.entity_mut::<String>(slice, "result")? =
                    request.downcast::<event_data::Record>().unwrap().text;
                Ok(Box::new(()))
            },
        )
    });
    for name in ["caller", "callee"] {
        let mut slice = ApplicationSlice::new(name);
        slice.bind("result", String::new()).unwrap();
        let slice = app.add_slice(slice).unwrap();
        let action = app.register_action(slice, descriptor).unwrap();
        env.action(
            &app,
            name,
            "record",
            app.slice(slice).unwrap().action_handle(action).unwrap(),
        )
        .unwrap();
    }
    let rows = app.create_entity(vec!["old".to_owned()]).unwrap();
    env.entity("rows", &app, rows).unwrap();
    let mut definition = compile("slot-events.pixui", r#"
        component Wrapper(children: Children) {<Column slice='callee'><Children /></Column>}
        component Echo(text: String) {<Button label=props.text @activate=caller.record(text: props.text) />}
        ui X {<Column slice='caller'>
            {for row in entities.rows => <Wrapper>
                <Echo text=row />
                <Button label=row @activate=record(text: row) />
            </Wrapper>}
        </Column>}
    "#, &env).unwrap().definition;
    struct Bindings(Vec<EventBinding>);
    impl Visitor for Bindings {
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
    let mut bindings = Bindings(vec![]);
    walk(
        &mut definition.template,
        LiveState::default().root_state_mut(),
        &ExpressionContext::new(&app),
        &mut bindings,
    )
    .unwrap();
    assert_eq!(bindings.0.len(), 2);
    app.resolve_mut(rows).unwrap()[0] = "new".into();
    for binding in bindings.0 {
        app.dispatch(binding(&app, &pixui_reflect::DynamicObject::from_reflect(())).unwrap())
            .unwrap();
        assert_eq!(
            app.entity::<String>(app.slice_named("caller").unwrap().id(), "result")
                .unwrap(),
            "old"
        );
        assert_eq!(
            app.entity::<String>(app.slice_named("callee").unwrap().id(), "result")
                .unwrap(),
            ""
        );
    }
}

#[test]
fn each_prop_is_evaluated_once_and_updates_preserve_mounted_state() {
    use crate::{
        components::label::LabelProps,
        expression::expression::Expression,
        live_model::{component::Component, state::PartState},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Probe;
    impl Component for Probe {
        type Props = LabelProps;
        type State = usize;
        type Events = crate::events::interface::NoEvents;
    }
    struct ProbeValues(Vec<(String, usize)>);
    impl Visitor for ProbeValues {
        fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
            if let (LivePart::Component(part), PartState::Component(state)) =
                (&*entry.part, &mut *entry.state)
            {
                let count = state.state.downcast_mut::<usize>().unwrap();
                *count += 1;
                self.0.push((
                    evaluate(entry.context, &part.expressions[0])?
                        .downcast_ref::<String>()
                        .unwrap()
                        .clone(),
                    *count,
                ));
            }
            Ok(())
        }
    }
    static EVALUATIONS: AtomicUsize = AtomicUsize::new(0);
    let (mut app, mut env) = fixture();
    env.component("Probe", app.register_component::<Probe>("probe").unwrap())
        .unwrap();
    env.entities.insert(
        "counter".into(),
        super::resolution::environment::DataExport {
            name: "counter".into(),
            descriptor: <String as pixui_reflect::Reflect>::type_descriptor(),
            expression: Expression::computed(|_| {
                Ok(pixui_reflect::DynamicObject::from_reflect(
                    EVALUATIONS.fetch_add(1, Ordering::SeqCst).to_string(),
                ))
            }),
        },
    );
    let mut root = compile("once.pixui", "component Pair(value: String) {<><Probe text=props.value /><Probe text=props.value /></>} ui X {<Pair value=entities.counter />}", &env).unwrap().into_template();
    let mut state = LiveState::default();
    EVALUATIONS.store(0, Ordering::SeqCst);
    for (walk_index, expected) in [(1, "0"), (2, "1")] {
        let mut values = ProbeValues(vec![]);
        walk(
            &mut root,
            state.root_state_mut(),
            &ExpressionContext::new(&app),
            &mut values,
        )
        .unwrap();
        assert_eq!(
            values.0,
            [(expected.into(), walk_index), (expected.into(), walk_index)]
        );
        assert_eq!(EVALUATIONS.load(Ordering::SeqCst), walk_index);
    }
}

#[test]
fn repeated_slot_occurrences_keep_outer_props_and_distinct_identity() {
    let (mut app, mut env) = fixture();
    let rows = app
        .create_entity(vec!["one".to_owned(), "two".to_owned()])
        .unwrap();
    env.entity("rows", &app, rows).unwrap();
    let source = r#"
        component Repeat(children: Children) {{for inner in entities.rows => <Children />}}
        component Outer(title: String) {<Repeat><Label text=props.title /></Repeat>}
        ui X {{for outer in entities.rows => <Outer title=outer />}}
    "#;
    assert_eq!(texts(&mut app, &env, source), ["one", "one", "two", "two"]);
    assert!(
        compile(
            "reserved.pixui",
            "ui X {{for props in entities.rows => <Label text='' />}}",
            &env
        )
        .err()
        .unwrap()
        .to_string()
        .contains("reserved")
    );
}
#[pixui_reflect::reflect]
mod aliases {
    #[derive(Clone)]
    pub struct Detail {
        pub title: String,
    }
}
#[test]
fn explicit_host_type_aliases_and_semantic_props_share_existing_conversions() {
    let (mut app, mut env) = fixture();
    env.prop_type::<aliases::Detail>("Detail").unwrap();
    let value = app
        .create_entity(aliases::Detail {
            title: "Nested".into(),
        })
        .unwrap();
    env.entity("detail", &app, value).unwrap();
    let title = app.localized_text("test", "Host title").unwrap();
    let title = app.create_entity(title).unwrap();
    env.entity("title", &app, title).unwrap();
    assert_eq!(
        texts(
            &mut app,
            &env,
            "component Title(value: Detail, localized: String) {<><Label text=props.value.title /><Label text=props.localized /></>} ui X {<Title localized=entities.title value=entities.detail />}"
        ),
        ["Nested", "Host title"]
    );
    assert!(compile("path.pixui", "component ImageView(path: ResourcePath) {<Image path=props.path />} ui X {<ImageView path='images/logo.png' />}", &env).is_ok());
    for path in ["\"images/logo.png\"", "'../logo.png'"] {
        assert!(compile("path.pixui", &format!("component ImageView(path: ResourcePath) {{<Image path=props.path />}} ui X {{<ImageView path={path} />}}"), &env).is_err());
    }
    assert!(env.prop_type::<String>("String").is_err());
    assert!(env.prop_type::<String>("Children").is_err());
}
#[test]
fn translation_declarations_cover_bodies_props_and_slots_without_double_translation() {
    use crate::i18n::catalog::{MessageKey, TranslationCatalog, TranslationEntry};
    let (mut app, env) = fixture();
    let source = r#"
        component Card(children: Children) {<Column><Label text="Body" /><Children /></Column>}
        component Title(value: String) {<Label text=props.value />}
        ui X domain 'test' {<><Card><Title value="Hello" /></Card><Card /></>}
    "#;
    let mut definition = compile("messages.pixui", source, &env).unwrap().definition;
    crate::i18n::registration::register(&mut definition, &mut app.translations).unwrap();
    let declarations = app.translations.declarations("test");
    assert_eq!(declarations.len(), 2);
    for message in declarations {
        assert_eq!(message.locations.len(), 1);
        assert!(
            message
                .locations
                .iter()
                .all(|location| location.contains("messages.pixui"))
        );
    }
    let language = app.translations.register_language("de").unwrap();
    app.translations
        .install(
            "test",
            language,
            TranslationCatalog {
                entries: [("Hello", "Hallo"), ("Body", "Inhalt")]
                    .into_iter()
                    .map(|(source, translation)| TranslationEntry {
                        key: MessageKey {
                            source: source.into(),
                            context: None,
                        },
                        translation: translation.into(),
                    })
                    .collect(),
                diagnostics: vec![],
            },
        )
        .unwrap();
    let mut values = Values(vec![]);
    walk(
        &mut definition.template,
        LiveState::default().root_state_mut(),
        &ExpressionContext::new(&app).with_language(language),
        &mut values,
    )
    .unwrap();
    assert_eq!(values.0, ["Inhalt", "Hallo", "Inhalt"]);
}
#[test]
fn declaration_limits_apply_across_library_and_entry_modules() {
    use super::{
        bundle::{BundleLimits, install},
        source::bundle::SourceBundle,
    };
    let (mut app, env) = fixture();
    let mut bundle = SourceBundle::new();
    bundle
        .add(
            "common.pixui",
            "export component A() {<></>} component Private() {<></>}",
        )
        .unwrap();
    bundle
        .add(
            "root.pixui",
            "use { A } from './common.pixui'; ui X {<A />}",
        )
        .unwrap();
    let error = install(
        &mut app,
        &bundle,
        &["root.pixui".into()],
        &env,
        "test",
        BundleLimits {
            declarations: 2,
            ..Default::default()
        },
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("declaration limit"));
    assert_eq!(app.template_count(), 0);
}
