//! Resource path checking and evaluation do not require image I/O or painters.
use super::{compiler::compile, diagnostic::report::Stage, resolution::environment::Environment};
use crate::{
    application::app::Application,
    components::image::{ImageComponent, ImageProps},
    expression::{context::ExpressionContext, evaluator::evaluate},
    live_model::part::LivePart,
    resources::path::ResourcePath,
};
use pixui_reflect::{DynamicObject, Reflect};
fn environment(app: &mut Application) -> Environment {
    let image = app.register_component::<ImageComponent>("image").unwrap();
    let mut env = Environment::new();
    env.component("Image", image).unwrap();
    env
}
fn path(app: &Application, env: &Environment, expression: &str) -> ResourcePath {
    let source = format!("ui X {{<Image path={expression} />}}");
    let compiled = compile("image.pixui", &source, env).unwrap();
    let LivePart::Component(part) = compiled.definition.template else {
        panic!()
    };
    evaluate(&ExpressionContext::new(app), &part.expressions[0])
        .unwrap()
        .downcast_ref::<ResourcePath>()
        .unwrap()
        .clone()
}
#[test]
fn resource_path_reflection_is_opaque_and_image_props_require_validated_values() {
    let descriptor = ResourcePath::type_descriptor();
    assert!(descriptor.fields().is_empty());
    assert!(descriptor.methods().is_empty());
    assert!(!descriptor.is_send_constructible());
    assert!(descriptor.construct(vec![]).is_err());
    let props = ImageProps::type_descriptor();
    assert_eq!(props.fields()[0].name, "path");
    assert_eq!(
        props.fields()[0].type_id(),
        Some(std::any::TypeId::of::<ResourcePath>())
    );
    let value = ResourcePath::new("Images/ä.png").unwrap();
    let constructed = props
        .construct_send(vec![Box::new(value.clone())])
        .unwrap()
        .downcast::<ImageProps>()
        .unwrap();
    assert_eq!(constructed.path, value);
    assert!(
        props
            .construct_send(vec![Box::new("../raw.png".to_owned())])
            .is_err()
    );
    assert!(props.construct_send(vec![]).is_err());
    assert!(
        props
            .construct_send(vec![Box::new(value.clone()), Box::new(value)])
            .is_err()
    );
    assert!(
        descriptor
            .construct(vec![DynamicObject::from_reflect("unchecked".to_owned())])
            .is_err()
    );
}
#[test]
fn literals_templates_strings_and_typed_paths_share_validation() {
    let mut app = Application::default();
    let mut env = environment(&mut app);
    for (source, expected) in [
        ("'Images/ä.png'", "Images/ä.png"),
        (r"'images/it\'s.png'", "images/it's.png"),
        ("'images/{{logo}}.png'", "images/{logo}.png"),
    ] {
        assert_eq!(path(&app, &env, source).as_str(), expected);
    }
    let name = app.create_entity("logo".to_owned()).unwrap();
    env.entity("name", &app, name).unwrap();
    assert_eq!(
        path(&app, &env, "'images/{entities.name}.png'").as_str(),
        "images/logo.png"
    );
    *app.resolve_mut(name).unwrap() = "other".into();
    assert_eq!(
        path(&app, &env, "'images/{entities.name}.png'").as_str(),
        "images/other.png"
    );
    let filename = app.create_entity("not-present.png".to_owned()).unwrap();
    env.entity("filename", &app, filename).unwrap();
    assert_eq!(
        path(&app, &env, "entities.filename").as_str(),
        "not-present.png"
    );
    let typed = app
        .create_entity(ResourcePath::new("typed.png").unwrap())
        .unwrap();
    env.entity("typed", &app, typed).unwrap();
    assert_eq!(path(&app, &env, "entities.typed").as_str(), "typed.png");
}
#[test]
fn invalid_literals_and_translated_subexpressions_have_checked_source_spans() {
    let mut app = Application::default();
    let env = environment(&mut app);
    let expressions = [
        "''",
        "'/logo.png'",
        "'../logo.png'",
        "'images/./logo.png'",
        "'images//logo.png'",
        "'images/'",
        r"'images\\logo.png'",
        "'C:logo.png'",
        "'a\0.png'",
        "true",
        "42",
        "\"images/logo.png\"",
        "'images/{\"logo\"}.png'",
    ];
    for expression in expressions {
        let source = format!("ui X {{<Image path={expression} />}}");
        let error = match compile("bad-image.pixui", &source, &env) {
            Ok(_) => panic!("accepted {expression}"),
            Err(error) => error,
        };
        assert!(
            error
                .diagnostics
                .iter()
                .all(|error| error.stage == Stage::Checking),
            "{error}"
        );
        for diagnostic in &error.diagnostics {
            assert!(diagnostic.primary.end <= source.len());
        }
        if expression.contains('"') {
            assert!(error.to_string().contains("single-quoted"), "{error}");
        }
    }
}
#[test]
fn dynamic_validation_preserves_source_origins() {
    let mut app = Application::default();
    let mut env = environment(&mut app);
    let filename = app.create_entity("../invalid.png".to_owned()).unwrap();
    env.entity("filename", &app, filename).unwrap();
    let compiled = compile(
        "dynamic-image.pixui",
        "ui X {<Image path=entities.filename />}",
        &env,
    )
    .unwrap();
    let LivePart::Component(part) = compiled.definition.template else {
        panic!()
    };
    let error = evaluate(&ExpressionContext::new(&app), &part.expressions[0])
        .err()
        .unwrap();
    assert!(error.to_string().contains("dynamic-image.pixui:1:"));
    assert!(
        error
            .to_string()
            .contains("invalid relative resource filename")
    );
}

#[pixui_reflect::reflect(send)]
mod custom {
    #[derive(Clone)]
    pub struct PathValue {
        pub path: crate::resources::path::ResourcePath,
    }
}
struct Preview;
impl crate::live_model::component::Component for Preview {
    type Props = custom::PathValue;
    type State = ();
    type Events = crate::events::interface::NoEvents;
}
#[test]
fn custom_props_and_callback_requests_use_the_same_contextual_conversion() {
    use crate::{
        application::{action::ActionDescriptor, application_slice::ApplicationSlice},
        components::text_input::TextInputComponent,
        events::{binding::Callback, standard::TextInputEventsChangePayload},
    };
    let mut app = Application::default();
    let mut env = Environment::new();
    env.component(
        "Preview",
        app.register_component::<Preview>("preview").unwrap(),
    )
    .unwrap();
    env.component(
        "Input",
        app.register_component::<TextInputComponent>("input")
            .unwrap(),
    )
    .unwrap();
    let mut slice = ApplicationSlice::new("images");
    slice.bind("result", String::new()).unwrap();
    let slice = app.add_slice(slice).unwrap();
    static ACTION: std::sync::OnceLock<ActionDescriptor> = std::sync::OnceLock::new();
    let action = ACTION.get_or_init(|| {
        ActionDescriptor::new::<custom::PathValue>(
            "select",
            "Select an image",
            vec![],
            |app, slice, request| {
                let request = request.downcast::<custom::PathValue>().unwrap();
                *app.entity_mut::<String>(slice, "result")? = request.path.as_str().into();
                Ok(Box::new(()))
            },
        )
    });
    let index = app.register_action(slice, action).unwrap();
    let handle = app.slice(slice).unwrap().action_handle(index).unwrap();
    env.action(&app, "images", "select", handle).unwrap();
    let compiled = compile("custom.pixui", "ui X {<Preview path='custom.png' />}", &env).unwrap();
    let LivePart::Component(part) = compiled.definition.template else {
        panic!()
    };
    assert_eq!(
        evaluate(&ExpressionContext::new(&app), &part.expressions[0])
            .unwrap()
            .downcast_ref::<ResourcePath>()
            .unwrap()
            .as_str(),
        "custom.png"
    );
    let compiled = compile(
        "callback-path.pixui",
        "ui X {<Input content='' @change=images.select(path: content) />}",
        &env,
    )
    .unwrap();
    let LivePart::Component(part) = compiled.definition.template else {
        panic!()
    };
    let Callback::Program(program) = &part.events[0].callback else {
        panic!()
    };
    // Preparing a published binding must not try to evaluate the future payload.
    let binding = program.prepare(&ExpressionContext::new(&app)).unwrap();
    let payload = |content: &str| {
        DynamicObject::from_reflect(TextInputEventsChangePayload {
            content: content.into(),
        })
    };
    app.dispatch(binding(&app, &payload("chosen.png")).unwrap())
        .unwrap();
    assert_eq!(app.entity::<String>(slice, "result").unwrap(), "chosen.png");
    let error = binding(&app, &payload("../bad.png")).err().unwrap();
    assert!(error.to_string().contains("callback-path.pixui"));
    assert_eq!(app.entity::<String>(slice, "result").unwrap(), "chosen.png");
}

#[test]
fn loop_field_paths_are_typed_and_do_not_register_translation_messages() {
    use crate::{
        components::label::LabelComponent,
        live_model::{
            state::LiveState,
            walk::{Visitor, WalkEntry, walk},
        },
    };
    let mut app = Application::default();
    let mut env = environment(&mut app);
    env.component(
        "Label",
        app.register_component::<LabelComponent>("label").unwrap(),
    )
    .unwrap();
    let rows = app
        .create_entity(vec![custom::PathValue {
            path: ResourcePath::new("Row/ä.png").unwrap(),
        }])
        .unwrap();
    env.entity("rows", &app, rows).unwrap();
    env.value_type::<custom::PathValue>();
    let mut definition = compile("loop-images.pixui", "ui X domain 'images' {<> <Label text=\"Images\" /> {for item in entities.rows => <Image path=item.path />} <Image path='images/{\'logo\'}.png' /> </>}", &env).unwrap().definition;
    crate::i18n::registration::register(&mut definition, &mut app.translations).unwrap();
    assert_eq!(app.translations.declarations("images").len(), 1);
    struct Paths(Vec<ResourcePath>);
    impl Visitor for Paths {
        fn visit(&mut self, entry: &mut WalkEntry) -> pixui_base::PixuiResult<()> {
            if let LivePart::Component(part) = &*entry.part {
                for expression in &part.expressions {
                    let value = evaluate(entry.context, expression)?;
                    if let Some(path) = value.downcast_ref::<ResourcePath>() {
                        self.0.push(path.clone());
                    }
                }
            }
            Ok(())
        }
    }
    let mut paths = Paths(vec![]);
    walk(
        &mut definition.template,
        LiveState::default().root_state_mut(),
        &ExpressionContext::new(&app),
        &mut paths,
    )
    .unwrap();
    assert_eq!(
        paths.0.iter().map(ResourcePath::as_str).collect::<Vec<_>>(),
        ["Row/ä.png", "images/logo.png"]
    );
}
