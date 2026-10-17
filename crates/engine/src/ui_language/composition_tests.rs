//! Reusable definitions retain independent mounted state and localized values.
use super::{compiler::compile, resolution::environment::Environment};
use crate::{
    application::app::Application,
    components::label::LabelComponent,
    composition::component_ref::ComponentRef,
    expression::{context::ExpressionContext, evaluator::evaluate, expression::Expression},
    live_model::{
        part::{DynamicPart, LivePart},
        state::{LiveState, PartState},
        walk::{Visitor, WalkEntry, walk},
    },
};
use pixui_base::PixuiResult;
use pixui_reflect::Reflect;
struct Collect(Vec<(String, crate::live_model::identity::ComponentPath)>);
impl Visitor for Collect {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
        if let LivePart::Component(part) = &*entry.part {
            self.0.push((
                evaluate(entry.context, &part.expressions[0])?
                    .downcast_ref::<String>()
                    .unwrap()
                    .clone(),
                entry.path.clone(),
            ));
        }
        Ok(())
    }
}
fn fixture() -> (Application, Environment) {
    let mut app = Application::default();
    let label = app.register_component::<LabelComponent>("label").unwrap();
    app.register_painter::<LabelComponent>(crate::painters::label::LabelPainter::default())
        .unwrap();
    let mut env = Environment::new();
    env.component("Label", label).unwrap();
    (app, env)
}
fn register(app: &mut Application, env: &Environment, text: &str) -> ComponentRef {
    let part = compile(
        "leaf.pixui",
        &format!("ui Leaf domain 'test' {{<Label text=\"{text}\" />}}"),
        env,
    )
    .unwrap()
    .into_template();
    app.register_template(text, "test", part).unwrap()
}
fn inspect(
    app: &Application,
    part: &mut LivePart,
    state: &mut LiveState,
) -> Vec<(String, crate::live_model::identity::ComponentPath)> {
    let mut collector = Collect(vec![]);
    walk(
        part,
        state.root_state_mut(),
        &ExpressionContext::new(app),
        &mut collector,
    )
    .unwrap();
    collector.0
}
#[test]
fn static_alias_and_dynamic_reference_share_the_runtime_and_reset_on_switch() {
    let (mut app, mut env) = fixture();
    let first = register(&mut app, &env, "First");
    let second = register(&mut app, &env, "Second");
    env.template("First", first).unwrap();
    let selected = app.create_entity(first).unwrap();
    env.entity("selected", &app, selected).unwrap();
    let mut part = compile(
        "shell.pixui",
        "ui Shell domain 'test' {<> <First /> <Dynamic component=entities.selected /> </>}",
        &env,
    )
    .unwrap()
    .into_template();
    let mut state = LiveState::default();
    let before = inspect(&app, &mut part, &mut state);
    assert_eq!(
        before.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
        ["First", "First"]
    );
    assert_ne!(before[0].1, before[1].1);
    assert_eq!(before, inspect(&app, &mut part, &mut state));
    *app.resolve_mut(selected).unwrap() = second;
    let after = inspect(&app, &mut part, &mut state);
    assert_eq!(after[1].0, "Second");
    assert_ne!(before[1].1, after[1].1);
    assert!(state.component_mut(&before[1].1).is_err());
    assert!(state.component_mut(&after[1].1).is_ok());
    assert!(ComponentRef::type_descriptor().fields().is_empty());
    assert!(!ComponentRef::type_descriptor().is_send_constructible());
}
#[test]
fn foreign_refs_domains_and_recursion_fail_before_unbounded_expansion() {
    let (mut app, env) = fixture();
    let reference = register(&mut app, &env, "First");
    let mut part = LivePart::Dynamic(DynamicPart::new(Expression::component(reference)));
    let (other, _) = fixture();
    assert!(
        walk(
            &mut part,
            LiveState::default().root_state_mut(),
            &ExpressionContext::new(&other),
            &mut Collect(vec![])
        )
        .is_err()
    );
    // Rust definitions can resolve references dynamically; cycles must be caught.
    let selected = app.create_entity(reference).unwrap();
    let recursive = app
        .register_template(
            "recursive",
            "test",
            LivePart::Dynamic(DynamicPart::new(Expression::entity(selected))),
        )
        .unwrap();
    *app.resolve_mut(selected).unwrap() = recursive;
    let mut cycle = LivePart::Dynamic(DynamicPart::new(Expression::component(recursive)));
    let error = walk(
        &mut cycle,
        LiveState::default().root_state_mut(),
        &ExpressionContext::new(&app),
        &mut Collect(vec![]),
    )
    .unwrap_err();
    assert!(error.to_string().contains("recursive"));
    let mut wrong = compile("wrong.pixui", "ui X domain 'other' {<></>}", &env)
        .unwrap()
        .definition;
    wrong.template = part;
    crate::i18n::registration::register(&mut wrong, &mut app.translations).unwrap();
    assert!(
        walk(
            &mut wrong.template,
            &mut PartState::Unknown,
            &ExpressionContext::new(&app),
            &mut Collect(vec![])
        )
        .is_err()
    );
}
#[test]
fn host_titles_are_exported_and_localized_without_translating_application_strings() {
    use crate::i18n::catalog::{MessageKey, TranslationCatalog, TranslationEntry};
    let (mut app, mut env) = fixture();
    let image = app
        .register_component::<crate::components::image::ImageComponent>("image")
        .unwrap();
    env.component("Image", image).unwrap();
    let text = app.localized_text("test", "Buttons").unwrap();
    let entity = app.create_entity(text).unwrap();
    env.entity("title", &app, entity).unwrap();
    let language = app.translations.register_language("de").unwrap();
    app.translations
        .install(
            "test",
            language,
            TranslationCatalog {
                entries: vec![TranslationEntry {
                    key: MessageKey {
                        source: "Buttons".into(),
                        context: None,
                    },
                    translation: "Schaltflächen".into(),
                }],
                diagnostics: vec![],
            },
        )
        .unwrap();
    let LivePart::Component(part) =
        compile("title.pixui", "ui X {<Label text=entities.title />}", &env)
            .unwrap()
            .into_template()
    else {
        panic!()
    };
    assert_eq!(
        evaluate(&ExpressionContext::new(&app), &part.expressions[0])
            .unwrap()
            .downcast_ref::<String>()
            .unwrap(),
        "Buttons"
    );
    assert_eq!(
        evaluate(
            &ExpressionContext::new(&app).with_language(language),
            &part.expressions[0]
        )
        .unwrap()
        .downcast_ref::<String>()
        .unwrap(),
        "Schaltflächen"
    );
    assert_eq!(app.translations.declarations("test").len(), 1);
    for source in [
        "ui X {<Image path=entities.title />}",
        "ui X {<Image path='images/{entities.title}.png' />}",
    ] {
        assert!(compile("localized-path.pixui", source, &env).is_err());
    }
    for source in [
        "ui X {<Dynamic />}",
        "ui X {<Dynamic component='file.pixui' />}",
        "ui X {<Dynamic component=entities.title><></></Dynamic>}",
    ] {
        assert!(compile("invalid.pixui", source, &env).is_err());
    }
}

#[test]
fn import_graphs_share_dependencies_keep_aliases_lexical_and_export_inactive_messages() {
    use super::{
        bundle::{BundleLimits, install},
        source::bundle::SourceBundle,
    };
    let (mut app, env) = fixture();
    let mut sources = SourceBundle::new();
    sources
        .add("common.pixui", "ui Common {<Label text=\"Shared\" />}")
        .unwrap();
    sources
        .add(
            "pages/one.pixui",
            "use Shared from '../common.pixui'; ui One {<Shared />}",
        )
        .unwrap();
    sources
        .add(
            "two.pixui",
            "use Shared from './common.pixui'; ui Two {<Shared />}",
        )
        .unwrap();
    sources.add("root.pixui", "use One from './pages/one.pixui'; use Two from './two.pixui'; ui Root {<> <One /> <Two /> </>}").unwrap();
    let installed = install(
        &mut app,
        &sources,
        &["root.pixui".into()],
        &env,
        "test",
        BundleLimits::default(),
    )
    .unwrap();
    assert_eq!(installed.len(), 4);
    assert_eq!(app.translations.declarations("test").len(), 1);
    let mut root = LivePart::Dynamic(DynamicPart::new(Expression::component(
        installed.component("root.pixui").unwrap(),
    )));
    let values = inspect(&app, &mut root, &mut LiveState::default());
    assert_eq!(
        values.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
        ["Shared", "Shared"]
    );
    assert_ne!(values[0].1, values[1].1);
    assert!(compile("outside.pixui", "ui X {<One />}", &env).is_err());
    assert!(
        compile(
            "single.pixui",
            "use One from './one.pixui'; ui X {<One />}",
            &env
        )
        .err()
        .unwrap()
        .to_string()
        .contains("resolver/bundle")
    );
}
#[test]
fn import_failures_are_source_located_and_transactional() {
    use super::{
        bundle::{BundleLimits, install},
        source::bundle::SourceBundle,
    };
    let (mut app, env) = fixture();
    for (root, child, message) in [
        (
            "use Child from './child.pixui'; ui X {<Child />}",
            "use Root from './root.pixui'; ui Child {<Root />}",
            "cyclic",
        ),
        (
            "use Child from './absent.pixui'; ui X {<Child />}",
            "ui Child {<></>}",
            "unknown source",
        ),
        (
            "use Child from '../escape.pixui'; ui X {<Child />}",
            "ui Child {<></>}",
            "escapes",
        ),
        (
            "use Label from './child.pixui'; ui X {<Label />}",
            "ui Child {<Label text=\"Uncommitted\" />}",
            "conflicting",
        ),
        (
            "use A from './child.pixui'; use A from './child.pixui'; ui X {<A />}",
            "ui Child {<></>}",
            "duplicate use",
        ),
        (
            "use A from './child.pixui'; ui X {<A />}",
            "ui Child {<Label text=true />}",
            "child.pixui",
        ),
        (
            "use A from './child.pixui'; ui X {<A />}",
            "ui Child domain 'other' {<></>}",
            "incompatible",
        ),
    ] {
        let mut bundle = SourceBundle::new();
        bundle.add("root.pixui", root).unwrap();
        bundle.add("child.pixui", child).unwrap();
        let error = install(
            &mut app,
            &bundle,
            &["root.pixui".into()],
            &env,
            "test",
            BundleLimits::default(),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains(message), "{error}");
        assert!(error.to_string().contains("root.pixui"), "{error}");
        assert!(app.translations.declarations("test").is_empty());
        assert_eq!(app.template_count(), 0);
    }
    for source in [
        "use A from \"child.pixui\"; ui X {<A />}",
        "use A from './{name}.pixui'; ui X {<A />}",
    ] {
        assert!(compile("bad-import.pixui", source, &env).is_err());
    }
    let mut bundle = SourceBundle::new();
    bundle
        .add("root.pixui", "use A from './child.pixui'; ui X {<A />}")
        .unwrap();
    bundle.add("child.pixui", "ui Child {<></>}").unwrap();
    for limits in [
        BundleLimits {
            modules: 1,
            ..Default::default()
        },
        BundleLimits {
            depth: 1,
            ..Default::default()
        },
        BundleLimits {
            source_bytes: 1,
            ..Default::default()
        },
    ] {
        assert!(
            install(
                &mut app,
                &bundle,
                &["root.pixui".into()],
                &env,
                "test",
                limits
            )
            .is_err()
        );
    }
}

struct Counter;
impl crate::live_model::component::Component for Counter {
    type Props = crate::components::label::LabelProps;
    type State = usize;
    type Events = crate::events::interface::NoEvents;
}
struct CounterPainter;
impl crate::painters::painter::Painter<Counter> for CounterPainter {
    fn measure(
        &self,
        _: &crate::painters::measure::MeasureContext<'_, Counter>,
    ) -> PixuiResult<crate::ui::geometry::Size> {
        Ok(crate::ui::geometry::Size {
            width: 1.0,
            height: 1.0,
        })
    }
    fn paint(
        &self,
        _: &mut crate::painters::context::PaintContext<'_, Counter>,
    ) -> PixuiResult<()> {
        Ok(())
    }
}
#[test]
fn mounted_state_is_independent_retained_and_reset_even_for_the_same_leaf_type() {
    let mut app = Application::default();
    let counter = app.register_component::<Counter>("counter").unwrap();
    app.register_painter::<Counter>(CounterPainter).unwrap();
    let mut env = Environment::new();
    env.component("Counter", counter).unwrap();
    let leaf = || {
        compile(
            "counter.pixui",
            "ui Counter {<Counter text='count' />}",
            &env,
        )
        .unwrap()
        .into_template()
    };
    let first = app.register_template("first", "test", leaf()).unwrap();
    let second = app.register_template("second", "test", leaf()).unwrap();
    let selected = app.create_entity(first).unwrap();
    env.entity("selected", &app, selected).unwrap();
    env.template("First", first).unwrap();
    let mut root = compile(
        "root.pixui",
        "ui Root {<> <First /> <Dynamic component=entities.selected /> </>}",
        &env,
    )
    .unwrap()
    .into_template();
    struct Count(Vec<usize>);
    impl Visitor for Count {
        fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
            if let PartState::Component(state) = entry.state {
                let value = state.state.downcast_mut::<usize>().unwrap();
                *value += 1;
                self.0.push(*value);
            }
            Ok(())
        }
    }
    let mut state = LiveState::default();
    for (reference, expected) in [
        (first, [1, 1]),
        (first, [2, 2]),
        (second, [3, 1]),
        (first, [4, 1]),
    ] {
        *app.resolve_mut(selected).unwrap() = reference;
        let mut values = Count(vec![]);
        walk(
            &mut root,
            state.root_state_mut(),
            &ExpressionContext::new(&app),
            &mut values,
        )
        .unwrap();
        assert_eq!(values.0, expected);
    }
}
#[test]
fn component_boundaries_clear_caller_loop_locals_and_bound_recursion_depth() {
    let (mut app, env) = fixture();
    let label = app.components().component::<LabelComponent>().unwrap();
    let leaf = crate::live_model::part::ComponentPart::typed_with_expressions(
        label,
        vec![Expression::computed(|context| {
            assert!(context.local(0).is_err());
            assert!(context.value().is_err());
            Ok(pixui_reflect::DynamicObject::from_reflect(
                "isolated".to_owned(),
            ))
        })],
        |_, _, values| {
            Ok(crate::components::label::LabelProps {
                text: values[0].downcast_ref::<String>().unwrap().clone(),
            })
        },
    );
    let reference = app
        .register_template("isolated", "test", LivePart::Component(leaf))
        .unwrap();
    let rows = app.create_entity(vec![1_u64, 2_u64]).unwrap();
    let mut part = LivePart::ForLoop(crate::live_model::part::ForLoopPart {
        key: None,
        expression: Expression::entity(rows),
        body: Box::new(LivePart::Dynamic(DynamicPart::new(Expression::component(
            reference,
        )))),
    });
    assert_eq!(inspect(&app, &mut part, &mut LiveState::default()).len(), 2);
    let mut reference = reference;
    for index in 0..64 {
        reference = app
            .register_template(
                format!("nested-{index}"),
                "test",
                LivePart::Dynamic(DynamicPart::new(Expression::component(reference))),
            )
            .unwrap();
    }
    let mut root = LivePart::Dynamic(DynamicPart::new(Expression::component(reference)));
    assert!(
        walk(
            &mut root,
            &mut PartState::Unknown,
            &ExpressionContext::new(&app),
            &mut Collect(vec![])
        )
        .unwrap_err()
        .to_string()
        .contains("nested")
    );
    let (mut other, _) = fixture();
    assert!(other.register_template("foreign", "test", root).is_err());
    assert!(
        app.register_template(
            "other-domain",
            "wrong",
            compile("empty.pixui", "ui X {<></>}", &env)
                .unwrap()
                .into_template()
        )
        .is_ok()
    );
}

#[pixui_reflect::reflect]
mod records {
    pub struct View {
        pub component: crate::composition::component_ref::ComponentRef,
    }
}
#[test]
fn collection_field_references_render_with_generational_occurrence_paths() {
    use crate::application::{application_slice::ApplicationSlice, collection::Collection};
    let (mut app, mut env) = fixture();
    let reference = register(&mut app, &env, "Collected");
    let mut collection = Collection::new_reflected::<records::View>("views");
    collection
        .arena_mut::<records::View>()
        .unwrap()
        .insert(records::View {
            component: reference,
        });
    collection
        .arena_mut::<records::View>()
        .unwrap()
        .insert(records::View {
            component: reference,
        });
    let collection = app.register_collection(collection).unwrap();
    let mut slice = ApplicationSlice::new("views");
    slice.bind_collection("views", collection).unwrap();
    app.add_slice(slice).unwrap();
    env.collection::<records::View>("views", &app, app.collection_key("views", "views").unwrap())
        .unwrap();
    let mut part = compile(
        "collection-views.pixui",
        "ui X {{for view in collections.views => <Dynamic component=view.component />}}",
        &env,
    )
    .unwrap()
    .into_template();
    let values = inspect(&app, &mut part, &mut LiveState::default());
    assert_eq!(
        values.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
        ["Collected", "Collected"]
    );
    assert_ne!(values[0].1, values[1].1);
}
#[test]
fn expanded_loop_limit_fails_before_allocating_mounted_item_states() {
    let app = &mut Application::default();
    let items = app.create_entity(vec![0_u64; 100_001]).unwrap();
    let mut part = LivePart::ForLoop(crate::live_model::part::ForLoopPart {
        key: None,
        expression: Expression::entity(items),
        body: Box::new(LivePart::Composite(
            crate::live_model::part::CompositePart { parts: vec![] },
        )),
    });
    let mut state = LiveState::default();
    let error = walk(
        &mut part,
        state.root_state_mut(),
        &ExpressionContext::new(app),
        &mut Collect(vec![]),
    )
    .unwrap_err();
    assert!(error.to_string().contains("node limit"));
    let PartState::ForLoop(state) = state.root_state() else {
        panic!()
    };
    assert!(state.items.is_empty());
}

#[pixui_reflect::reflect(send)]
mod requests {
    pub struct Title {
        pub text: String,
    }
}
#[test]
fn deferred_localized_text_keeps_the_owning_definition_domain() {
    use crate::{
        application::{action::ActionDescriptor, application_slice::ApplicationSlice},
        components::button::ButtonComponent,
        events::binding::Callback,
    };
    let (mut app, mut env) = fixture();
    env.component(
        "Button",
        app.register_component::<ButtonComponent>("button").unwrap(),
    )
    .unwrap();
    let title = app.localized_text("other", "Foreign title").unwrap();
    let reference = app.create_entity(title).unwrap();
    env.entity("title", &app, reference).unwrap();
    let slice = app.add_slice(ApplicationSlice::new("test")).unwrap();
    static ACTION: std::sync::OnceLock<ActionDescriptor> = std::sync::OnceLock::new();
    let action = ACTION.get_or_init(|| {
        ActionDescriptor::new::<requests::Title>("record", "test", vec![], |_, _, _| {
            Ok(Box::new(()))
        })
    });
    let action = app.register_action(slice, action).unwrap();
    env.action(
        &app,
        "test",
        "record",
        app.slice(slice).unwrap().action_handle(action).unwrap(),
    )
    .unwrap();
    let LivePart::Component(part) = compile(
        "title-event.pixui",
        "ui X domain 'test' {<Button label='' @activate=test.record(text: entities.title) />}",
        &env,
    )
    .unwrap()
    .into_template() else {
        panic!()
    };
    let Callback::Program(program) = &part.events[0].callback else {
        panic!()
    };
    let binding = program
        .prepare(&ExpressionContext::new(&app).with_domain(Some("test")))
        .unwrap();
    let error = binding(&app, &pixui_reflect::DynamicObject::from_reflect(()))
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .contains("incompatible translation domain")
    );
}
