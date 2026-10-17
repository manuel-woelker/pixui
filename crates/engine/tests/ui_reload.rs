//! Native filesystem changes compile on the loader and reset live UI instances.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::{
        app::Application, application_handle::ApplicationHandle,
        application_slice::ApplicationSlice, entity_mut::EntityMut,
    },
    expression::expression::Expression,
    live_model::{
        part::{DynamicPart, LivePart},
        state::PartState,
    },
    resources::{
        directory::DirectoryFilesystem,
        filesystem::{ResourceFilesystem, ResourceReader},
        reload::{builder::ResourceReloadBuilder, session::ResourceReloadSession},
    },
    ui::{
        definition::{UiDefinition, UiDefinitionId},
        display_list::{DrawCommand, RenderOutput},
        focus::ComponentInstanceId,
        input::{Key, KeyboardEvent, UiCommand, UiInput},
        instance::UiInstanceId,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
        text_input::protocol::{ClipboardReply, HostEffect},
        window_properties::WindowCommand,
    },
    ui_language::{
        bundle::{BundleLimits, InstalledBundle},
        resolution::environment::Environment,
        source::bundle::SourceBundle,
    },
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
const TIMEOUT: Duration = Duration::from_secs(8);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "pixui-ui-reload-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, contents: &str) {
        std::fs::write(self.0.join(path), contents).unwrap();
    }
    fn filesystem(&self) -> Arc<dyn ResourceFilesystem> {
        Arc::new(DirectoryFilesystem::new(&self.0).unwrap())
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn texts(output: &RenderOutput) -> Vec<&str> {
    output
        .display_list
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::StyledText { text, .. } | DrawCommand::DrawText { text, .. } => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect()
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !condition() {
        assert!(Instant::now() < deadline, "UI reload condition timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[track_caller]
fn output(receiver: &OutputReceiver, expected: &str) -> RenderOutput {
    let deadline = Instant::now() + TIMEOUT;
    let mut last = vec![];
    loop {
        if let Ok(frame) = receiver.try_recv() {
            if texts(&frame).contains(&expected) {
                return frame;
            }
            last = texts(&frame).into_iter().map(str::to_owned).collect();
        }
        assert!(
            Instant::now() < deadline,
            "UI reload timed out awaiting {expected:?}, last text: {last:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[pixui_engine::application::action::slice_actions(slice="reload", facade=Actions)]
mod actions {
    use super::*;
    #[action]
    pub fn edit(mut content: EntityMut<String>, value: String) {
        *content = value;
    }
    #[action]
    pub fn increment(mut count: EntityMut<u64>) {
        *count += 1;
    }
}
const MAIN: &str = "use {Card} from './common.pixui'; use Body from './body.pixui'; ui Main {<Column slice='reload'><Card title=\"Heading\"><Body /></Card><Button label='Press' @activate=increment() /><TextInput content=entities.content @change=edit(value:content) /></Column>}";
const LIB: &str = "export component Card(title: String, children: Children) {<Column><Label text=title /><Children /></Column>}";
const BODY: &str = "ui Body {<Label text='Body' />}";
struct Fixture {
    app: ApplicationHandle,
    installed: InstalledBundle,
    definition: UiDefinitionId,
    first: UiInstanceId,
    second: UiInstanceId,
    a: OutputReceiver,
    b: OutputReceiver,
    directory: Directory,
}
impl Fixture {
    fn new() -> Self {
        let app = Application::new();
        let components = app.register_standard_components().unwrap();
        app.register_standard_painters().unwrap();
        let mut slice = ApplicationSlice::new("reload");
        slice.bind("content", "retained".to_owned()).unwrap();
        slice.bind("count", 0u64).unwrap();
        let slice = app.add_slice(slice).unwrap();
        actions::Actions::register(&app, slice).unwrap();
        let env = app
            .inspect(move |app| {
                let mut env = Environment::new();
                env.component("Label", components.label)?;
                env.component("Button", components.button)?;
                env.component("TextInput", components.text_input)?;
                env.component("Image", components.image)?;
                env.entity("content", app, app.entity_ref::<String>(slice, "content")?)?;
                for name in ["increment", "edit"] {
                    env.action(
                        app,
                        "reload",
                        name,
                        app.slice(slice)?.action_handle_named(name)?,
                    )?;
                }
                Ok(env)
            })
            .unwrap();
        let mut sources = SourceBundle::new();
        for (path, text) in [
            ("main.pixui", MAIN),
            ("common.pixui", LIB),
            ("body.pixui", BODY),
        ] {
            sources.add(path, text).unwrap();
        }
        let installed = app
            .install_bundle(
                sources,
                vec!["main.pixui".into()],
                env,
                "reload",
                BundleLimits::default(),
            )
            .unwrap();
        let definition = app
            .register_ui(UiDefinition::new(
                "reload",
                LivePart::Dynamic(DynamicPart::new(Expression::component(
                    installed.component("main.pixui").unwrap(),
                ))),
            ))
            .unwrap();
        let (first, a) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let (second, b) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        output(&a, "Heading");
        output(&b, "Heading");
        let directory = Directory::new();
        for (path, text) in [
            ("main.pixui", MAIN),
            ("common.pixui", LIB),
            ("body.pixui", BODY),
        ] {
            directory.write(path, text);
        }
        Self {
            app,
            installed,
            definition,
            first,
            second,
            a,
            b,
            directory,
        }
    }
    fn start(&self) -> ResourceReloadSession {
        self.start_with(self.directory.filesystem())
    }
    fn start_with(&self, fs: Arc<dyn ResourceFilesystem>) -> ResourceReloadSession {
        ResourceReloadBuilder::new(self.app.clone())
            .quiet_period(Duration::from_millis(30))
            .ui_bundle(fs, &self.installed)
            .unwrap()
            .start()
            .unwrap()
    }
    fn focus(&self) -> ComponentInstanceId {
        let id = self.first;
        self.app
            .inspect(move |app| {
                let instance = app.uis().instance(id)?;
                instance.component_id(instance.layout().text_inputs[0].component_index)
            })
            .unwrap()
    }
}
#[test]
fn imported_graph_reloads_both_windows_preserves_model_and_rejects_old_input() {
    let f = Fixture::new();
    let session = f.start();
    session.wait_initial(TIMEOUT).unwrap();
    let initial = output(&f.a, "Heading");
    output(&f.b, "Heading");
    let old_target = f.focus();
    f.app
        .ui_command(UiCommand::Focus {
            instance: f.first,
            target: Some(old_target.clone()),
        })
        .unwrap();
    let mut old_session = None;
    while let Ok(command) = f.a.window_commands().try_recv() {
        if let WindowCommand::SetTextInput(input) = command {
            old_session = input.session;
        }
    }
    let old_session = old_session.unwrap();
    f.app
        .ui_command(UiCommand::HostAttached {
            instance: f.first,
            clipboard: true,
        })
        .unwrap();
    let mut paste = KeyboardEvent::named("paste");
    paste.key = Key::Character("v".into());
    paste.modifiers.control = true;
    f.app
        .ui_command(UiCommand::TextInput {
            instance: f.first,
            revision: initial.revision,
            session: Some(old_session),
            input: Box::new(UiInput::Keyboard(paste)),
        })
        .unwrap();
    let HostEffect::ReadClipboard { request } = f.a.effects().recv_timeout(TIMEOUT).unwrap() else {
        panic!("paste effect")
    };
    f.app
        .ui_command(UiCommand::Visibility {
            instance: f.second,
            visible: false,
        })
        .unwrap();
    let baseline = f.app.inspect(|app| Ok(app.template_count())).unwrap();
    f.directory.write("common.pixui", "export component Card(title: String, children: Children) {<Column><Label text='{title} revised' /><Children /></Column>}");
    let changed = output(&f.a, "Heading revised");
    assert!(changed.revision > initial.revision);
    assert!(texts(&initial).contains(&"Heading"));
    assert!(
        f.app
            .ui_command(UiCommand::Focus {
                instance: f.first,
                target: Some(old_target)
            })
            .is_err()
    );
    assert!(
        f.app
            .ui_command(UiCommand::Input {
                instance: f.first,
                revision: initial.revision,
                input: UiInput::Keyboard(KeyboardEvent::named("Enter"))
            })
            .is_err()
    );
    assert!(
        f.app
            .ui_command(UiCommand::TextInput {
                instance: f.first,
                revision: initial.revision,
                session: Some(old_session),
                input: Box::new(UiInput::ImeCommit("stale".into()))
            })
            .is_err()
    );
    f.app
        .ui_command(UiCommand::Clipboard(ClipboardReply {
            instance: f.first,
            request,
            result: Ok(Some("stale paste".into())),
        }))
        .unwrap();
    let second = f.second;
    let definition = f.definition;
    f.app
        .inspect(move |app| {
            assert!(matches!(
                app.uis().instance(second)?.state().root_state(),
                PartState::Unknown
            ));
            assert!(
                app.uis()
                    .instance(second)?
                    .layout()
                    .component_bounds
                    .is_empty()
            );
            assert!(app.uis().definition(definition)?.state().focus.is_none());
            assert_eq!(
                app.entity::<String>(app.slice_named("reload")?.id(), "content")?,
                "retained"
            );
            Ok(())
        })
        .unwrap();
    f.app
        .ui_command(UiCommand::Visibility {
            instance: f.second,
            visible: true,
        })
        .unwrap();
    output(&f.b, "Heading revised");
    for version in 0..4 {
        f.directory.write(
            "body.pixui",
            &format!("ui Body {{<Label text='Body {version}' />}}"),
        );
        output(&f.a, &format!("Body {version}"));
        output(&f.b, &format!("Body {version}"));
    }
    assert_eq!(
        f.app.inspect(|app| Ok(app.template_count())).unwrap(),
        baseline
    );
    actions::Actions::bind(&f.app).unwrap().increment().unwrap();
    assert_eq!(
        f.app
            .inspect(|app| Ok(*app.entity::<u64>(app.slice_named("reload")?.id(), "count")?))
            .unwrap(),
        1
    );
    session.stop();
}
#[test]
fn invalid_files_and_new_imports_recover_without_discarding_last_good_state() {
    let f = Fixture::new();
    let session = f.start();
    session.wait_initial(TIMEOUT).unwrap();
    let initial = output(&f.a, "Heading");
    output(&f.b, "Heading");
    let target = f.focus();
    let before = f.app.inspect(|app| Ok(app.template_count())).unwrap();
    f.directory.write("common.pixui", "export component Card(");
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(f.focus(), target);
    assert_eq!(
        f.app.inspect(|app| Ok(app.template_count())).unwrap(),
        before
    );
    assert!(f.a.try_recv().is_err());
    f.directory.write("common.pixui", LIB);
    f.directory.write(
        "main.pixui",
        &MAIN.replace("'./body.pixui'", "'./new.pixui'"),
    );
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(f.focus(), target);
    f.directory
        .write("new.pixui", "ui New {<Label text='Created' />}");
    let created = output(&f.a, "Created");
    assert!(created.revision > initial.revision);
    let before_failure = f.focus();
    f.directory
        .write("new.pixui", "ui New {<Image path='missing.png' />}");
    wait(|| {
        let id = f.first;
        f.app
            .inspect(move |app| Ok(app.uis().instance(id)?.last_error().is_some()))
            .unwrap()
    });
    assert!(
        f.a.try_recv().is_err(),
        "runtime failure keeps the published frame"
    );
    assert!(
        f.app
            .ui_command(UiCommand::Focus {
                instance: f.first,
                target: Some(before_failure)
            })
            .is_err()
    );
    f.directory
        .write("new.pixui", "ui New {<Label text='Recovered' />}");
    output(&f.a, "Recovered");
    session.stop();
    f.directory
        .write("new.pixui", "ui New {<Label text='Stopped' />}");
    std::thread::sleep(Duration::from_millis(100));
    f.app.inspect(|_| Ok(())).unwrap();
    assert!(f.a.try_recv().is_err());
}
struct ThreadCheckingFilesystem {
    inner: Arc<dyn ResourceFilesystem>,
    seen: Arc<Mutex<Vec<String>>>,
    block: Mutex<
        Option<(
            crossbeam_channel::Sender<()>,
            crossbeam_channel::Receiver<()>,
        )>,
    >,
}
impl ResourceFilesystem for ThreadCheckingFilesystem {
    fn open(
        &self,
        path: &pixui_engine::resources::path::ResourcePath,
    ) -> PixuiResult<Option<ResourceReader>> {
        let name = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_owned();
        self.seen.lock().unwrap().push(name);
        if let Some((entered, release)) = self.block.lock().unwrap().take() {
            entered.send(()).unwrap();
            release.recv_timeout(TIMEOUT).unwrap();
        }
        self.inner.open(path)
    }
    fn watch_roots(&self) -> Vec<PathBuf> {
        self.inner.watch_roots()
    }
}
#[test]
fn blocked_background_loading_does_not_block_actions_or_inspection() {
    let f = Fixture::new();
    let seen = Arc::new(Mutex::new(vec![]));
    let (entered, waiting) = crossbeam_channel::bounded(1);
    let (release, blocked) = crossbeam_channel::bounded(1);
    let fs = Arc::new(ThreadCheckingFilesystem {
        inner: f.directory.filesystem(),
        seen: seen.clone(),
        block: Mutex::new(Some((entered, blocked))),
    });
    let session = f.start_with(fs);
    waiting.recv_timeout(TIMEOUT).unwrap();
    actions::Actions::bind(&f.app).unwrap().increment().unwrap();
    f.app.inspect(|_| Ok(())).unwrap();
    release.send(()).unwrap();
    session.wait_initial(TIMEOUT).unwrap();
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .all(|name| name == "pixui-resource-loader")
    );
    session.stop();
}

#[test]
fn ui_catalog_and_image_updates_share_one_session_and_reuse_existing_translations() {
    use pixui_engine::{
        i18n::po::PoFormat,
        resources::{image_loader::ImageLoader, path::ResourcePath},
        ui::presentation::Theme,
    };
    let f = Fixture::new();
    let german = f.app.register_language("de").unwrap();
    f.directory.write(
        "de.po",
        "msgid \"Heading\"\nmsgstr \"Kopf\"\n\nmsgid \"Known\"\nmsgstr \"Bekannt\"\n",
    );
    let mut png = vec![];
    {
        let mut encoder = png::Encoder::new(&mut png, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[10, 20, 30])
            .unwrap();
    }
    std::fs::write(f.directory.0.join("logo.png"), png).unwrap();
    f.app
        .set_image_loader(ImageLoader::new(f.directory.filesystem()))
        .unwrap();
    let settings = f
        .app
        .presentation_language(
            PresentationSettings {
                theme: Theme::Dark,
                ..Default::default()
            },
            "de",
        )
        .unwrap();
    f.app
        .ui_command(UiCommand::Present {
            instance: f.second,
            settings,
        })
        .unwrap();
    let session = ResourceReloadBuilder::new(f.app.clone())
        .quiet_period(Duration::from_millis(30))
        .watch_images()
        .catalog(
            f.directory.filesystem(),
            ResourcePath::new("de.po").unwrap(),
            "reload",
            german,
            Arc::new(PoFormat),
        )
        .unwrap()
        .ui_bundle(f.directory.filesystem(), &f.installed)
        .unwrap()
        .start()
        .unwrap();
    session.wait_initial(TIMEOUT).unwrap();
    output(&f.a, "Heading");
    output(&f.b, "Kopf");
    f.directory.write(
        "body.pixui",
        "ui Body {<Column><Label text=\"Known\" /><Image path='logo.png' /></Column>}",
    );
    let first = output(&f.a, "Known");
    let second = output(&f.b, "Bekannt");
    assert_eq!(first.display_list.images.len(), 1);
    assert_eq!(second.display_list.images.len(), 1);
    let target = f.focus();
    f.directory.write(
        "de.po",
        "msgid \"Heading\"\nmsgstr \"Kopf\"\n\nmsgid \"Known\"\nmsgstr \"Neu\"\n",
    );
    output(&f.b, "Neu");
    assert_eq!(
        f.focus(),
        target,
        "catalog reload does not reset UI identity"
    );
    // An identical source save must not reset identity or allocate more slots.
    f.directory.write(
        "body.pixui",
        "ui Body {<Column><Label text=\"Known\" /><Image path='logo.png' /></Column>}",
    );
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(f.focus(), target);
    session.stop();
}

#[test]
fn initial_source_failure_keeps_session_alive_for_recovery() {
    let f = Fixture::new();
    f.directory
        .write("common.pixui", "export component Broken(");
    let target = f.focus();
    let session = f.start();
    assert!(session.wait_initial(TIMEOUT).is_err());
    assert!(session.is_running());
    assert_eq!(f.focus(), target);
    f.directory.write("common.pixui", LIB);
    output(&f.a, "Heading");
    assert_ne!(f.focus(), target);
    session.stop();
}

#[test]
fn layered_source_overrides_atomic_renames_and_deletion_keep_canonical_identity() {
    use pixui_engine::resources::layered::LayeredFilesystem;
    let f = Fixture::new();
    let upper = Directory::new();
    let fs = Arc::new(LayeredFilesystem::new(vec![
        upper.filesystem(),
        f.directory.filesystem(),
    ]));
    let session = f.start_with(fs);
    session.wait_initial(TIMEOUT).unwrap();
    output(&f.a, "Heading");
    upper.write("pending", "export component Card(title: String, children: Children) {<Column><Label text='Override {title}' /><Children /></Column>}");
    std::fs::rename(upper.0.join("pending"), upper.0.join("common.pixui")).unwrap();
    output(&f.a, "Override Heading");
    let current = f.focus();
    std::fs::write(upper.0.join("common.pixui"), [0xff]).unwrap();
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(f.focus(), current);
    std::fs::remove_file(upper.0.join("common.pixui")).unwrap();
    output(&f.a, "Heading");
    let id = f.installed.id();
    let root = f.installed.component("main.pixui").unwrap();
    assert_eq!(
        f.app
            .inspect(move |app| app.installed_bundle(id)?.component("main.pixui"))
            .unwrap(),
        root
    );
    session.stop();
}

#[test]
fn stopping_source_reload_does_not_wait_for_a_busy_worker() {
    let f = Fixture::new();
    let session = f.start();
    session.wait_initial(TIMEOUT).unwrap();
    output(&f.a, "Heading");
    let (entered, waiting) = crossbeam_channel::bounded(1);
    let (release, blocked) = crossbeam_channel::bounded(1);
    let app = f.app.clone();
    let worker = std::thread::spawn(move || {
        app.inspect(move |_| {
            entered.send(()).unwrap();
            blocked.recv_timeout(TIMEOUT).unwrap();
            Ok(())
        })
    });
    waiting.recv_timeout(TIMEOUT).unwrap();
    f.directory
        .write("body.pixui", "ui Body {<Label text='Waiting' />}");
    std::thread::sleep(Duration::from_millis(150));
    let (stopped, done) = crossbeam_channel::bounded(1);
    let stopping = std::thread::spawn(move || {
        session.stop();
        stopped.send(()).unwrap();
    });
    let result = done.recv_timeout(Duration::from_secs(2));
    release.send(()).unwrap();
    worker.join().unwrap().unwrap();
    stopping.join().unwrap();
    assert!(
        result.is_ok(),
        "loader shutdown waited for worker snapshot delivery"
    );
}
