//! End-to-end compiler/runtime contracts without native windows or font loading.
use super::{compiler::compile, diagnostic::report::Stage, resolution::environment::Environment};
use crate::painters::painter::Painter;
use crate::{
    application::{
        action::ActionDescriptor, app::Application, application_slice::ApplicationSlice,
        collection::Collection,
    },
    components::{
        button::{ButtonComponent, ButtonProps},
        checkbox::CheckboxComponent,
        label::{LabelComponent, LabelProps},
    },
    expression::{context::ExpressionContext, evaluator::evaluate},
    live_model::{
        part::LivePart,
        state::{LiveState, PartState},
        walk::{Visitor, WalkEntry, walk},
    },
    ui::{activation::ActionBinding, presentation::PresentationSettings},
};
use pixui_base::{Key, PixuiResult};

#[pixui_reflect::reflect(send)]
mod data {
    #[derive(Clone)]
    pub struct Detail {
        pub name: String,
    }
    #[derive(Clone)]
    pub struct Item {
        pub title: String,
        pub done: bool,
        pub detail: Detail,
        pub children: Vec<Detail>,
    }
    pub struct MarkRequest {
        pub item: crate::application::object_ref::ObjectRef<Item>,
    }
    pub struct CountProps {
        pub value: u8,
    }
}
struct CountComponent;
impl crate::live_model::component::Component for CountComponent {
    type Events = crate::events::interface::NoEvents;
    type Props = data::CountProps;
    type State = ();
}
fn mark_done_descriptor() -> &'static ActionDescriptor {
    static DESCRIPTOR: std::sync::OnceLock<ActionDescriptor> = std::sync::OnceLock::new();
    DESCRIPTOR.get_or_init(|| {
        ActionDescriptor::new::<data::MarkRequest>(
            "mark_done",
            "mark an item",
            vec![],
            |app, _, request| {
                let request = request.downcast::<data::MarkRequest>().unwrap();
                app.resolve_mut(request.item)?.done = true;
                Ok(Box::new(()))
            },
        )
    })
}

struct Fixture {
    app: Application,
    env: Environment,
    key: Key<data::Item>,
    collection: crate::application::collection_key::CollectionKey,
}
fn fixture() -> Fixture {
    let mut app = Application::default();
    let label = app.register_component::<LabelComponent>("label").unwrap();
    let button = app.register_component::<ButtonComponent>("button").unwrap();
    let checkbox = app
        .register_component::<CheckboxComponent>("checkbox")
        .unwrap();
    let count = app.register_component::<CountComponent>("count").unwrap();
    let slice = app.add_slice(ApplicationSlice::new("items")).unwrap();
    let mut collection = Collection::new_reflected::<data::Item>("todos");
    let key = collection
        .arena_mut::<data::Item>()
        .unwrap()
        .insert(data::Item {
            title: "First".into(),
            done: false,
            detail: data::Detail {
                name: "Parent".into(),
            },
            children: vec![data::Detail {
                name: "Child".into(),
            }],
        });
    let collection = app.add_collection(slice, collection).unwrap();
    let index = app.register_action(slice, mark_done_descriptor()).unwrap();
    let handle = app.slice(slice).unwrap().action_handle(index).unwrap();
    let reference = app.object_ref_at(collection, key).unwrap();
    let mut env = Environment::new();
    env.component("Label", label).unwrap();
    env.component("Button", button).unwrap();
    env.component("Checkbox", checkbox).unwrap();
    env.component("Count", count).unwrap();
    env.value_type::<data::Detail>();
    env.value_type::<Vec<data::Detail>>();
    env.collection::<data::Item>("todos", &app, collection)
        .unwrap();
    env.entity("first", &app, reference).unwrap();
    env.action(&app, "items", "mark_done", handle).unwrap();
    Fixture {
        app,
        env,
        key,
        collection,
    }
}
#[derive(Default)]
struct Inspect {
    texts: Vec<String>,
    actions: Vec<ActionBinding>,
    paths: Vec<crate::live_model::identity::ComponentPath>,
}
impl Visitor for Inspect {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
        if let LivePart::Component(part) = entry.part {
            let values = part
                .expressions
                .iter()
                .map(|expression| evaluate(entry.context, expression))
                .collect::<PixuiResult<Vec<_>>>()?;
            let PartState::Component(state) = entry.state else {
                panic!()
            };
            let props = part.binding.as_ref().unwrap().prepare(
                entry.context,
                &PresentationSettings::default(),
                &mut state.state,
                &values,
            )?;
            if let Some(label) = props.downcast_ref::<LabelProps>() {
                self.texts.push(label.text.clone());
            }
            if let Some(button) = props.downcast_ref::<ButtonProps>() {
                self.texts.push(button.label.clone());
            }
            for callback in &part.events {
                if let crate::events::binding::Callback::Program(program) = &callback.callback {
                    let binding = program.prepare(entry.context)?;
                    self.actions.push(Box::new(move |app| {
                        binding(
                            app,
                            &pixui_reflect::DynamicObject::from_reflect(
                                crate::events::standard::ActivationEventsActivatePayload {},
                            ),
                        )
                    }));
                }
            }
            self.paths.push(entry.path.clone());
        }
        Ok(())
    }
}
fn inspect(
    fixture: &Fixture,
    definition: &mut crate::ui::definition::UiDefinition,
    state: &mut LiveState,
) -> PixuiResult<Inspect> {
    let mut visitor = Inspect::default();
    walk(
        &mut definition.template,
        state.root_state_mut(),
        &ExpressionContext::new(&fixture.app),
        &mut visitor,
    )?;
    Ok(visitor)
}
fn register_messages(fixture: &mut Fixture, definition: &mut crate::ui::definition::UiDefinition) {
    crate::i18n::registration::register(definition, &mut fixture.app.translations).unwrap();
}
#[test]
fn compiled_loops_fields_templates_match_and_deferred_actions_work() {
    let mut f = fixture();
    let source = r#"ui Test {<Column gap=8>
        {for todo in collections.todos => <>
            <Label text="Todo: {todo.title}" />
            <Label text='Raw {todo.detail.name} {{ok}}' />
            <Button label="Done" @activate=items.mark_done(item: todo) />
            {match todo.done; true => <Label text="Completed" />, false => <Label text="Open" />,}
            {for child in todo.children => <Label text='{todo.title}: {child.name}' />}
        </>}
    </Column>}"#;
    let mut definition = compile("todo.pixui", source, &f.env).unwrap().definition;
    register_messages(&mut f, &mut definition);
    let mut state = LiveState::default();
    let first = inspect(&f, &mut definition, &mut state).unwrap();
    assert_eq!(
        first.texts,
        [
            "Todo: First",
            "Raw Parent {ok}",
            "Done",
            "Open",
            "First: Child"
        ]
    );
    assert!(
        !f.app
            .resolve_collection(f.collection)
            .unwrap()
            .arena::<data::Item>()
            .unwrap()
            .get(f.key)
            .unwrap()
            .done
    );
    let call = first.actions[0](&f.app).unwrap();
    f.app.dispatch(call).unwrap();
    let second = inspect(&f, &mut definition, &mut state).unwrap();
    assert_eq!(second.texts[3], "Completed");
    assert_eq!(first.paths[0], second.paths[0]);
    f.app
        .resolve_collection_mut::<data::Item>(f.collection)
        .unwrap()
        .remove(f.key);
    assert!(f.app.dispatch(first.actions[0](&f.app).unwrap()).is_err());
}
#[test]
fn translation_is_default_and_untranslated_templates_are_not_exported() {
    let mut f = fixture();
    let mut definition=compile("i18n.pixui",r#"ui X domain 'test' {<>
        <Label text="Hello {entities.first.title}" />
        <Label text='Raw {entities.first.title}' />
        {match entities.first.done; true => <Label text="Hidden" />, false => <Label text="Visible" />,}
    </>}"#,&f.env).unwrap().definition;
    register_messages(&mut f, &mut definition);
    let catalog = f.app.translations.declarations("test");
    assert_eq!(catalog.len(), 3);
    assert!(
        catalog
            .iter()
            .any(|entry| entry.key.source == "Hello {entities_first_title}")
    );
    assert!(
        !catalog
            .iter()
            .any(|entry| entry.key.source.starts_with("Raw"))
    );
    assert!(catalog.iter().all(|entry| {
        entry
            .locations
            .iter()
            .any(|location| location.starts_with("i18n.pixui:"))
    }));
}
#[test]
fn errors_are_precise_at_all_stages() {
    let f = fixture();
    for (source, stage, fragment) in [
        ("ui X {<Label text=$ />}", Stage::Lexer, "$"),
        ("ui X {<Label text='x'></Button>}", Stage::Parser, "Button"),
        ("ui X {<Unknown />}", Stage::Resolution, "Unknown"),
        ("ui X {<Label unknown='x' />}", Stage::Resolution, "unknown"),
        (
            "ui X {<Label text=entities.first.unknown />}",
            Stage::Resolution,
            "unknown",
        ),
        ("ui X {<Label text=true />}", Stage::Checking, "true"),
        ("ui X {<Count value=256 />}", Stage::Checking, "256"),
        (
            "ui X {<Label text=items.mark_done(item: entities.first) />}",
            Stage::Checking,
            "items.mark_done(item: entities.first)",
        ),
    ] {
        let errors = match compile("error.pixui", source, &f.env) {
            Err(errors) => errors,
            Ok(_) => panic!("unexpected success"),
        };
        assert_eq!(errors.diagnostics[0].stage, stage, "{errors}");
        assert_eq!(
            errors.source.slice(errors.diagnostics[0].primary),
            Some(fragment)
        );
        assert!(errors.to_string().contains("error.pixui:"));
    }
}
#[test]
fn rejects_invalid_loop_match_and_template_contracts() {
    let f = fixture();
    for source in [
        "ui X {{for x in entities.first.done => <Label text='x' />}}",
        "ui X {{match entities.first.done; true => <Label text='x' />,}}",
        "ui X {{match entities.first.done; true => <Label text='x' />, true => <Label text='y' />, false => <Label text='z' />,}}",
        "ui X {{match entities.first.done; _ => <Label text='x' />, true => <Label text='y' />,}}",
        "ui X {<Label text='x'><Label text='y'/></Label>}",
        "ui X {<Checkbox label='x' />}",
        "ui X {<Label text='x' text='y'/>}",
        "ui X {<Label text=entities.first />}",
        "ui X {<Label text=\"{collections.todos}\" />}",
        "ui X {<Button label='x' @activate=items.mark_done(item: true) />}",
        "ui X {<Row gap=-1/>}",
        "ui X {<Grid columns='bad'/>}",
    ] {
        assert!(
            compile("invalid.pixui", source, &f.env).is_err(),
            "{source}"
        );
    }
}
#[test]
fn failed_resolution_collects_independent_sibling_errors() {
    let f = fixture();
    let errors = match compile("siblings.pixui", "ui X {<><Missing/><Absent/></>}", &f.env) {
        Err(errors) => errors,
        Ok(_) => panic!(),
    };
    assert_eq!(errors.diagnostics.len(), 2);
}
#[test]
fn runtime_errors_retain_source_and_foreign_bindings_are_rejected() {
    let mut f = fixture();
    let mut definition = compile(
        "stale.pixui",
        "ui X {<Label text=entities.first.title />}",
        &f.env,
    )
    .unwrap()
    .definition;
    register_messages(&mut f, &mut definition);
    f.app
        .resolve_collection_mut::<data::Item>(f.collection)
        .unwrap()
        .remove(f.key);
    let error = inspect(&f, &mut definition, &mut LiveState::default())
        .err()
        .unwrap();
    assert!(error.to_string().contains("stale.pixui:1:"));
    let other = Application::default();
    assert!(
        evaluate(
            &ExpressionContext::new(&other),
            &f.env.collections["todos"].expression
        )
        .is_err()
    );
}
#[test]
fn layout_grid_and_exported_constants_are_validated() {
    let mut f = fixture();
    f.env
        .layout(
            "wide",
            crate::layout::style::LayoutStyle::fixed(100.0, 20.0),
        )
        .unwrap();
    let compiled=compile("grid.pixui","ui X {<Grid columns='100px 1fr auto' rows='auto' gap=4><Label layout=layouts.wide text='x' /></Grid>}",&f.env).unwrap();
    let LivePart::Container(grid) = compiled.definition.template else {
        panic!()
    };
    assert_eq!(grid.gap, 4.0);
    let LivePart::Component(label) = &grid.children[0] else {
        panic!()
    };
    assert_eq!(
        label.layout.width,
        crate::layout::style::Length::Pixels(100.0)
    );
}
#[test]
fn numeric_literals_use_expected_type_and_preserve_exact_equality() {
    let f = fixture();
    assert!(compile("n.pixui", "ui X {<Count value=255/>}", &f.env).is_ok());
    assert!(compile("n.pixui", "ui X {<Count value=-1/>}", &f.env).is_err());
    assert!(compile("n.pixui", "ui X {<Count value=1.5/>}", &f.env).is_err());
    assert!(
        compile(
            "n.pixui",
            "ui X {<Row gap=99999999999999999999999999999999999999999999999999999/>}",
            &f.env
        )
        .is_err()
    );
}

#[test]
fn pure_logical_expressions_short_circuit_stale_entity_reads() {
    let mut f = fixture();
    let mut definition = compile(
        "short.pixui",
        "ui X {<Checkbox label='x' checked=true || entities.first.done />}",
        &f.env,
    )
    .unwrap()
    .definition;
    register_messages(&mut f, &mut definition);
    f.app
        .resolve_collection_mut::<data::Item>(f.collection)
        .unwrap()
        .remove(f.key);
    assert!(inspect(&f, &mut definition, &mut LiveState::default()).is_ok());
}
#[test]
fn independent_type_errors_are_collected_without_lowering() {
    let f = fixture();
    let errors = match compile(
        "types.pixui",
        "ui X {<><Label text=true/><Count value=256/></>}",
        &f.env,
    ) {
        Err(errors) => errors,
        Ok(_) => panic!(),
    };
    assert_eq!(errors.diagnostics.len(), 2);
    assert!(
        errors
            .diagnostics
            .iter()
            .all(|error| error.stage == Stage::Checking)
    );
}
#[test]
fn placeholders_reuse_paths_and_reject_collisions() {
    let mut f = fixture();
    let mut definition = compile(
        "repeat.pixui",
        r#"ui X {<Label text="{entities.first.title} {entities.first.title}" />}"#,
        &f.env,
    )
    .unwrap()
    .definition;
    register_messages(&mut f, &mut definition);
    let declaration = f.app.translations.declarations("X");
    assert_eq!(declaration[0].placeholders.len(), 1);
    assert_eq!(
        inspect(&f, &mut definition, &mut LiveState::default())
            .unwrap()
            .texts,
        ["First First"]
    );
}
#[test]
fn general_sequence_loop_items_cannot_become_entity_references() {
    let mut f = fixture();
    let slice = f.app.slice_named("items").unwrap().id();
    f.app
        .bind(
            slice,
            "list",
            vec![data::Item {
                title: "vector".into(),
                done: false,
                detail: data::Detail {
                    name: "name".into(),
                },
                children: vec![],
            }],
        )
        .unwrap();
    let reference = f.app.entity_ref::<Vec<data::Item>>(slice, "list").unwrap();
    f.env.entity("list", &f.app, reference).unwrap();
    assert!(compile("refs.pixui","ui X {{for item in entities.list => <Button label='x' @activate=items.mark_done(item: item)/>}}",&f.env).is_err());
}
#[test]
fn installation_is_transactional_and_errors_retain_source() {
    let mut f = fixture();
    let mut compiled = compile(
        "install.pixui",
        "ui Installed {<Label text=\"New message\"/>}",
        &f.env,
    )
    .unwrap();
    // No painter exists, so neither a definition nor a message may be installed.
    let before = f.app.translations.declarations("Installed").len();
    let error = compiled.install(&mut f.app).unwrap_err();
    assert!(error.to_string().contains("install.pixui:1:1"));
    assert!(f.app.uis().definition_id("Installed").is_err());
    assert_eq!(f.app.translations.declarations("Installed").len(), before);
    crate::painters::label::LabelPainter::default()
        .register(&mut f.app.painters, &f.app.components, &mut f.app.styles)
        .unwrap();
    compiled = compile(
        "install.pixui",
        "ui Installed {<Label text=\"New message\"/>}",
        &f.env,
    )
    .unwrap();
    compiled.install(&mut f.app).unwrap();
    assert_eq!(f.app.translations.declarations("Installed").len(), 1);
}
