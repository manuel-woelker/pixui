//! Metadata delivery is independent of frame replacement and visibility.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::{
        action::slice_actions, app::Application, application_handle::ApplicationHandle,
        application_slice::ApplicationSlice, entity_mut::EntityMut,
    },
    expression::context::ExpressionContext,
    live_model::part::{CompositePart, LivePart},
    resources::{directory::DirectoryFilesystem, image_loader::ImageLoader, path::ResourcePath},
    ui::{
        definition::UiDefinition,
        input::UiCommand,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
        window_properties::{WindowCommand, WindowProperties},
    },
};
use std::{sync::Arc, time::Duration};
#[pixui_reflect::reflect]
mod model {
    pub struct Metadata {
        pub title: String,
        pub icon: bool,
        pub bad: bool,
    }
}
#[slice_actions(slice = "window", facade = WindowActions)]
mod actions {
    use super::*;
    #[action]
    pub fn configure(
        mut metadata: EntityMut<model::Metadata>,
        title: String,
        icon: bool,
        bad: bool,
    ) {
        metadata.title = title;
        metadata.icon = icon;
        metadata.bad = bad;
    }
}
fn properties(
    context: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<WindowProperties> {
    let app = context.application()?;
    let metadata = app.entity::<model::Metadata>(app.slice_named("window")?.id(), "metadata")?;
    Ok(WindowProperties {
        title: format!("{} ({})", metadata.title, settings.locale).into(),
        icon: if metadata.icon {
            Some(ResourcePath::new(if metadata.bad {
                "missing.png"
            } else {
                "images/pixui-logo.png"
            })?)
        } else {
            None
        },
    })
}
fn setup() -> (
    ApplicationHandle,
    actions::WindowActions,
    pixui_engine::ui::definition::UiDefinitionId,
) {
    let app = Application::new();
    app.set_image_loader(ImageLoader::new(Arc::new(
        DirectoryFilesystem::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets")).unwrap(),
    )))
    .unwrap();
    let mut slice = ApplicationSlice::new("window");
    slice
        .bind(
            "metadata",
            model::Metadata {
                title: "Initial".into(),
                icon: true,
                bad: false,
            },
        )
        .unwrap();
    let slice = app.add_slice(slice).unwrap();
    actions::WindowActions::register(&app, slice).unwrap();
    let actions = actions::WindowActions::bind(&app).unwrap();
    let definition = app
        .register_ui(
            UiDefinition::new(
                "window",
                LivePart::Composite(CompositePart { parts: vec![] }),
            )
            .with_window_properties(properties),
        )
        .unwrap();
    (app, actions, definition)
}
fn barrier(app: &ApplicationHandle) {
    app.inspect(|_| Ok(())).unwrap();
}
fn drain(outputs: &OutputReceiver) -> Vec<WindowCommand> {
    let mut commands = Vec::new();
    while let Ok(command) = outputs.window_commands().try_recv() {
        commands.push(command);
    }
    commands
}
#[test]
fn changed_fields_only_hidden_updates_icon_clear_and_errors() {
    let (app, actions, definition) = setup();
    let (instance, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    outputs.recv_timeout(Duration::from_secs(2)).unwrap();
    barrier(&app);
    let initial = drain(&outputs);
    assert_eq!(initial.len(), 2);
    assert_eq!(initial[0], WindowCommand::SetTitle("Initial (en)".into()));
    assert!(matches!(initial[1], WindowCommand::SetIcon(Some(_))));
    actions.configure("Changed", true, false).unwrap();
    barrier(&app);
    assert_eq!(
        drain(&outputs),
        [WindowCommand::SetTitle("Changed (en)".into())]
    );
    actions.configure("Changed", true, false).unwrap();
    barrier(&app);
    assert!(drain(&outputs).is_empty());
    // A frame is pending, but metadata slots do not share that mailbox.
    outputs.try_recv().unwrap();
    app.ui_command(UiCommand::Visibility {
        instance,
        visible: false,
    })
    .unwrap();
    actions.configure("Hidden", true, false).unwrap();
    barrier(&app);
    assert_eq!(
        drain(&outputs),
        [WindowCommand::SetTitle("Hidden (en)".into())]
    );
    assert!(outputs.try_recv().is_err());
    actions.configure("Hidden", false, false).unwrap();
    barrier(&app);
    assert_eq!(drain(&outputs), [WindowCommand::SetIcon(None)]);
    actions.configure("Must not publish", true, true).unwrap();
    barrier(&app);
    assert!(drain(&outputs).is_empty());
    app.inspect(move |app| {
        assert!(
            app.uis()
                .instance(instance)?
                .window_properties_error()
                .unwrap()
                .contains("missing.png")
        );
        Ok(())
    })
    .unwrap();
    actions.configure("Recovered", false, false).unwrap();
    barrier(&app);
    assert_eq!(
        drain(&outputs),
        [WindowCommand::SetTitle("Recovered (en)".into())]
    );
    app.inspect(move |app| {
        assert!(
            app.uis()
                .instance(instance)?
                .window_properties_error()
                .is_none()
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn slow_consumer_keeps_latest_title_and_pending_icon_and_instances_localize() {
    let (app, actions, definition) = setup();
    let (_, one) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let (_, two) = app
        .create_ui(
            definition,
            PresentationSettings {
                locale: "de".into(),
                ..Default::default()
            },
        )
        .unwrap();
    actions.configure("Second", true, false).unwrap();
    actions.configure("Latest", true, false).unwrap();
    barrier(&app);
    let one = drain(&one);
    let two = drain(&two);
    assert_eq!(one.len(), 2);
    assert_eq!(two.len(), 2);
    assert_eq!(one[0], WindowCommand::SetTitle("Latest (en)".into()));
    assert_eq!(two[0], WindowCommand::SetTitle("Latest (de)".into()));
    assert_eq!(one[1], two[1], "both windows reuse the same icon snapshot");
}
#[test]
fn no_resolver_keeps_host_defaults_and_closing_disconnects_commands() {
    let app = Application::new();
    let definition = app
        .register_ui(UiDefinition::new(
            "plain",
            LivePart::Composite(CompositePart { parts: vec![] }),
        ))
        .unwrap();
    let (instance, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    barrier(&app);
    assert!(drain(&outputs).is_empty());
    app.ui_command(UiCommand::Close { instance }).unwrap();
    assert_eq!(
        outputs.window_commands().try_recv(),
        Err(crossbeam_channel::TryRecvError::Disconnected)
    );
}
