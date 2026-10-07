//! Native directory notifications exercise the same image/catalog pipeline.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::{app::Application, application_handle::ApplicationHandle},
    components::{image::ImageProps, label::LabelProps},
    expression::{context::ExpressionContext, expression::Expression},
    i18n::{
        catalog::{MessageDeclaration, TranslationCatalog},
        format::TranslationFormat,
        po::PoFormat,
    },
    live_model::part::{ComponentPart, CompositePart, LivePart},
    resources::{
        directory::DirectoryFilesystem, filesystem::ResourceFilesystem, image_loader::ImageLoader,
        layered::LayeredFilesystem, path::ResourcePath, reload::builder::ResourceReloadBuilder,
    },
    ui::{
        definition::UiDefinition,
        display_list::{DrawCommand, RenderOutput},
        image::Image,
        input::UiCommand,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
        window_properties::{WindowCommand, WindowProperties},
    },
};
use pixui_reflect::DynamicObject;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(5);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "pixui-reload-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn filesystem(&self) -> Arc<dyn ResourceFilesystem> {
        Arc::new(DirectoryFilesystem::new(&self.0).unwrap())
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        std::fs::write(self.0.join(name), bytes).unwrap();
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn png(red: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[red, 0, 0, 128])
            .unwrap();
    }
    bytes
}
fn catalog(text: &str) -> String {
    format!("msgid \"Save\"\nmsgstr \"{text}\"\n")
}
fn builder(app: &ApplicationHandle) -> ResourceReloadBuilder {
    ResourceReloadBuilder::new(app.clone()).quiet_period(Duration::from_millis(30))
}
fn until<T>(mut read: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(value) = read() {
            return value;
        }
        assert!(Instant::now() < deadline, "resource update timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn image(app: &ApplicationHandle, name: &str) -> Option<Image> {
    let path = ResourcePath::new(name.to_owned()).unwrap();
    app.inspect(move |app| app.load_image(&path)).ok()
}
fn red(image: &Image) -> u8 {
    match image.pixels() {
        pixui_engine::ui::image::ImagePixels::Rgba { pixels } => pixels[0].0,
        _ => panic!("expected RGBA"),
    }
}
fn wait_image(app: &ApplicationHandle, name: &str, expected: u8) -> Image {
    until(|| image(app, name).filter(|image| red(image) == expected))
}
fn label(
    _: &ExpressionContext<'_>,
    _: &PresentationSettings,
    values: &[DynamicObject<'_>],
) -> PixuiResult<LabelProps> {
    Ok(LabelProps {
        text: values[0]
            .downcast_ref::<String>()
            .ok_or_else(|| pixui_error!("expected text"))?
            .clone(),
    })
}
fn props(_: &ExpressionContext<'_>, _: &PresentationSettings) -> PixuiResult<ImageProps> {
    ImageProps::new("logo.png")
}
fn window(
    _: &ExpressionContext<'_>,
    _: &PresentationSettings,
    values: &[DynamicObject<'_>],
) -> PixuiResult<WindowProperties> {
    Ok(WindowProperties {
        title: values[0].downcast_ref::<String>().unwrap().clone().into(),
        icon: Some(ResourcePath::new("logo.png")?),
    })
}
fn definition(app: &ApplicationHandle) -> pixui_engine::ui::definition::UiDefinitionId {
    let components = app.register_standard_components().unwrap();
    app.register_standard_painters().unwrap();
    app.register_ui(
        UiDefinition::new(
            "test",
            LivePart::Composite(CompositePart {
                parts: vec![
                    LivePart::Component(ComponentPart::typed_with_expressions(
                        components.label,
                        vec![Expression::text("Save").unwrap()],
                        label,
                    )),
                    LivePart::Component(ComponentPart::typed(components.image, props)),
                ],
            }),
        )
        .with_window_property_expressions(vec![Expression::text("Save").unwrap()], window),
    )
    .unwrap()
}
fn output(outputs: &OutputReceiver, text: &str, color: u8) -> RenderOutput {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let frame = outputs
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        if frame.display_list.commands.iter().any(
            |command| matches!(command, DrawCommand::DrawText { text: value, .. } if value == text),
        ) && frame
            .display_list
            .images
            .iter()
            .any(|image| red(image) == color)
        {
            return frame;
        }
    }
}
struct ObservedFormat(crossbeam_channel::Sender<String>);
impl TranslationFormat for ObservedFormat {
    fn import(&self, input: &str) -> PixuiResult<TranslationCatalog> {
        self.0
            .send(
                std::thread::current()
                    .name()
                    .unwrap_or("unnamed")
                    .to_owned(),
            )
            .unwrap();
        PoFormat.import(input)
    }
    fn export(&self, messages: &[MessageDeclaration]) -> PixuiResult<String> {
        PoFormat.export(messages)
    }
}

#[test]
fn images_and_po_reload_together_and_preserve_old_outputs() {
    let dir = Directory::new();
    dir.write("logo.png", png(10));
    dir.write("de.po", catalog("Speichern"));
    let app = Application::new();
    app.set_image_loader(ImageLoader::new(dir.filesystem()))
        .unwrap();
    let definition = definition(&app);
    let german = app.register_language("de").unwrap();
    let (parsed, parses) = crossbeam_channel::unbounded();
    let reload = builder(&app)
        .watch_images()
        .catalog(
            dir.filesystem(),
            ResourcePath::new("de.po").unwrap(),
            "test",
            german,
            Arc::new(ObservedFormat(parsed)),
        )
        .unwrap()
        .start()
        .unwrap();
    reload.wait_initial(TIMEOUT).unwrap();
    assert_eq!(
        parses.recv_timeout(TIMEOUT).unwrap(),
        "pixui-resource-loader"
    );
    let mut instances = Vec::new();
    let mut outputs = Vec::new();
    for _ in 0..2 {
        let (instance, receiver) = app
            .create_ui(
                definition,
                PresentationSettings {
                    language: german,
                    ..Default::default()
                },
            )
            .unwrap();
        instances.push(instance);
        outputs.push(receiver);
    }
    let old = output(&outputs[0], "Speichern", 10);
    output(&outputs[1], "Speichern", 10);
    dir.write("next.png", png(20));
    std::fs::rename(dir.0.join("next.png"), dir.0.join("logo.png")).unwrap();
    dir.write("next.po", catalog("Sichern"));
    std::fs::rename(dir.0.join("next.po"), dir.0.join("de.po")).unwrap();
    let new = output(&outputs[0], "Sichern", 20);
    output(&outputs[1], "Sichern", 20);
    assert_eq!(red(old.display_list.images.iter().next().unwrap()), 10);
    assert!(!Image::ptr_eq(
        old.display_list.images.iter().next().unwrap(),
        new.display_list.images.iter().next().unwrap()
    ));
    until(|| {
        outputs[0].window_commands().try_recv().ok().filter(
            |command| matches!(command, WindowCommand::SetTitle(title) if title == "Sichern"),
        )
    });
    // A parser failure cannot replace the valid catalog. An invalid image also
    // keeps its current snapshot. The next valid edits recover automatically.
    while parses.try_recv().is_ok() {}
    dir.write("de.po", "msgid \"Save\"\nmsgstr \"unterminated");
    parses.recv_timeout(TIMEOUT).unwrap();
    dir.write("logo.png", b"incomplete PNG");
    assert_eq!(red(&image(&app, "logo.png").unwrap()), 20);
    app.ui_command(UiCommand::Visibility {
        instance: instances[1],
        visible: false,
    })
    .unwrap();
    while outputs[1].window_commands().try_recv().is_ok() {}
    dir.write("de.po", catalog("Gespeichert"));
    dir.write("logo.png", png(30));
    output(&outputs[0], "Gespeichert", 30);
    until(|| {
        outputs[1].window_commands().try_recv().ok().filter(
            |command| matches!(command, WindowCommand::SetTitle(title) if title == "Gespeichert"),
        )
    });
    until(|| {
        outputs[1].window_commands().try_recv().ok().filter(
            |command| matches!(command, WindowCommand::SetIcon(Some(image)) if red(image) == 30),
        )
    });
    assert!(outputs[1].try_recv().is_err());
    app.ui_command(UiCommand::Visibility {
        instance: instances[1],
        visible: true,
    })
    .unwrap();
    output(&outputs[1], "Gespeichert", 30);
    reload.stop();
}

#[test]
fn layered_overrides_new_files_removal_and_recovery() {
    let low = Directory::new();
    let high = Directory::new();
    low.write("logo.png", png(1));
    let app = Application::new();
    app.set_image_loader(ImageLoader::new(Arc::new(LayeredFilesystem::new(vec![
        high.filesystem(),
        low.filesystem(),
    ]))))
    .unwrap();
    let reload = builder(&app).watch_images().start().unwrap();
    reload.wait_initial(TIMEOUT).unwrap();
    let original = wait_image(&app, "logo.png", 1);
    high.write("logo.png", png(2));
    let overridden = wait_image(&app, "logo.png", 2);
    // A shadowed lower layer edit causes a read, but identical winning bytes
    // preserve allocation identity rather than decoding a new version.
    low.write("logo.png", png(3));
    high.write("new.png", png(5));
    assert!(image(&app, "new.png").is_none());
    wait_image(&app, "new.png", 5);
    assert!(Image::ptr_eq(
        &overridden,
        &image(&app, "logo.png").unwrap()
    ));
    std::fs::remove_file(high.0.join("logo.png")).unwrap();
    wait_image(&app, "logo.png", 3);
    // Completely missing files keep the last successful snapshot.
    std::fs::remove_file(low.0.join("logo.png")).unwrap();
    high.write("new.png", png(6));
    wait_image(&app, "new.png", 6);
    assert_eq!(red(&image(&app, "logo.png").unwrap()), 3);
    high.write("logo.png", b"invalid present override");
    low.write("logo.png", png(7));
    high.write("new.png", png(8));
    wait_image(&app, "new.png", 8);
    assert_eq!(red(&image(&app, "logo.png").unwrap()), 3);
    high.write("logo.png", png(9));
    wait_image(&app, "logo.png", 9);
    assert_eq!(red(&original), 1);
    reload.stop();
}

#[test]
fn missing_catalog_can_be_created_and_domain_replacement_is_atomic() {
    let dir = Directory::new();
    let app = Application::new();
    let de = app.register_language("de").unwrap();
    let reload = builder(&app)
        .catalog(
            dir.filesystem(),
            ResourcePath::new("de.po").unwrap(),
            "test",
            de,
            Arc::new(PoFormat),
        )
        .unwrap()
        .start()
        .unwrap();
    assert!(reload.wait_initial(TIMEOUT).is_err());
    assert!(reload.is_running());
    dir.write("de.po", catalog("Speichern"));
    // Register after loading: retained stable keys fill the new declaration.
    let components = app.register_standard_components().unwrap();
    app.register_standard_painters().unwrap();
    let ui = app
        .register_ui(UiDefinition::new(
            "test",
            LivePart::Component(ComponentPart::typed_with_expressions(
                components.label,
                vec![Expression::text("Save").unwrap()],
                label,
            )),
        ))
        .unwrap();
    let (_, outputs) = app
        .create_ui(
            ui,
            PresentationSettings {
                language: de,
                ..Default::default()
            },
        )
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let frame = outputs
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        if frame.display_list.commands.iter().any(
            |command| matches!(command, DrawCommand::DrawText { text, .. } if text == "Speichern"),
        ) {
            break;
        }
    }
    reload.stop();
}

#[test]
fn disabled_by_default_and_stopping_releases_watch_only_snapshots() {
    let dir = Directory::new();
    dir.write("logo.png", png(1));
    let app = Application::new();
    app.set_image_loader(ImageLoader::new(dir.filesystem()))
        .unwrap();
    let old = image(&app, "logo.png").unwrap();
    dir.write("logo.png", png(2));
    assert!(Image::ptr_eq(&old, &image(&app, "logo.png").unwrap()));
    let reload = builder(&app).watch_images().start().unwrap();
    reload.wait_initial(TIMEOUT).unwrap();
    let current = wait_image(&app, "logo.png", 2);
    let weak = current.downgrade();
    drop(current);
    assert!(weak.upgrade().is_some());
    reload.stop();
    assert!(weak.upgrade().is_none());
    dir.write("logo.png", png(3));
    assert_eq!(red(&image(&app, "logo.png").unwrap()), 3); // ordinary synchronous loading resumes
    assert_eq!(red(&old), 1);
    // A stopped session can be replaced without keeping the application alive.
    builder(&app).watch_images().start().unwrap().stop();
}

#[test]
fn stopping_cancels_delivery_even_when_the_worker_queue_is_full() {
    let dir = Directory::new();
    dir.write("logo.png", png(1));
    let app = Application::with_capacity(0);
    let (opened, reads) = crossbeam_channel::unbounded();
    app.set_image_loader(ImageLoader::new(Arc::new(ObservedFilesystem {
        inner: dir.filesystem(),
        opened,
    })))
    .unwrap();
    let reload = builder(&app).watch_images().start().unwrap();
    reload.wait_initial(TIMEOUT).unwrap();
    wait_image(&app, "logo.png", 1);
    while reads.try_recv().is_ok() {}
    let (entered, blocked) = crossbeam_channel::bounded(1);
    let (release, gate) = crossbeam_channel::bounded(1);
    let handle = app.clone();
    let blocker = std::thread::spawn(move || {
        handle.inspect(move |_| {
            entered.send(()).unwrap();
            gate.recv().unwrap();
            Ok(())
        })
    });
    blocked.recv_timeout(TIMEOUT).unwrap();
    dir.write("logo.png", png(2));
    reads.recv_timeout(TIMEOUT).unwrap();
    let start = Instant::now();
    reload.stop();
    assert!(start.elapsed() < TIMEOUT);
    release.send(()).unwrap();
    blocker.join().unwrap().unwrap();
}

#[test]
fn invalid_startup_and_duplicate_sessions_are_rejected() {
    let app = Application::new();
    assert!(builder(&app).start().is_err());
    assert!(builder(&app).watch_images().start().is_err());
    let dir = Directory::new();
    app.set_image_loader(ImageLoader::new(dir.filesystem()))
        .unwrap();
    assert!(
        builder(&app)
            .watch_images()
            .quiet_period(Duration::ZERO)
            .start()
            .is_err()
    );
    let reload = builder(&app).watch_images().start().unwrap();
    assert!(builder(&app).watch_images().start().is_err());
    reload.stop();
    let de = app.register_language("de").unwrap();
    let configured = builder(&app)
        .catalog(
            dir.filesystem(),
            ResourcePath::new("de.po").unwrap(),
            "test",
            de,
            Arc::new(PoFormat),
        )
        .unwrap();
    assert!(
        configured
            .catalog(
                dir.filesystem(),
                ResourcePath::new("other.po").unwrap(),
                "test",
                de,
                Arc::new(PoFormat)
            )
            .is_err()
    );
}

struct ObservedFilesystem {
    inner: Arc<dyn ResourceFilesystem>,
    opened: crossbeam_channel::Sender<(String, String)>,
}
impl ResourceFilesystem for ObservedFilesystem {
    fn open(
        &self,
        path: &ResourcePath,
    ) -> PixuiResult<Option<pixui_engine::resources::filesystem::ResourceReader>> {
        self.opened
            .send((
                path.as_str().to_owned(),
                std::thread::current()
                    .name()
                    .unwrap_or("unnamed")
                    .to_owned(),
            ))
            .unwrap();
        self.inner.open(path)
    }
    fn watch_roots(&self) -> Vec<PathBuf> {
        self.inner.watch_roots()
    }
}

#[test]
fn lazy_image_subscription_reads_only_requested_paths_on_background_thread() {
    let dir = Directory::new();
    dir.write("unused.png", png(1));
    let (opened, reads) = crossbeam_channel::unbounded();
    let app = Application::new();
    app.set_image_loader(ImageLoader::new(Arc::new(ObservedFilesystem {
        inner: dir.filesystem(),
        opened,
    })))
    .unwrap();
    let reload = builder(&app).watch_images().start().unwrap();
    reload.wait_initial(TIMEOUT).unwrap();
    assert!(image(&app, "created.png").is_none());
    let (path, thread) = reads.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(path, "created.png");
    assert_eq!(thread, "pixui-resource-loader");
    dir.write("unused.png", png(2));
    dir.write("created.png", png(3));
    wait_image(&app, "created.png", 3);
    for (path, thread) in reads.try_iter() {
        assert_eq!(path, "created.png");
        assert_eq!(thread, "pixui-resource-loader");
    }
    reload.stop();
}

struct PausedFormat {
    block: Arc<std::sync::atomic::AtomicBool>,
    entered: crossbeam_channel::Sender<()>,
    release: crossbeam_channel::Receiver<()>,
}
impl TranslationFormat for PausedFormat {
    fn import(&self, input: &str) -> PixuiResult<TranslationCatalog> {
        if self.block.swap(false, Ordering::AcqRel) {
            self.entered.send(()).unwrap();
            self.release.recv().unwrap();
        }
        PoFormat.import(input)
    }
    fn export(&self, messages: &[MessageDeclaration]) -> PixuiResult<String> {
        PoFormat.export(messages)
    }
}
#[test]
fn edits_during_background_parsing_discard_the_obsolete_snapshot() {
    let dir = Directory::new();
    dir.write("de.po", catalog("Initial"));
    let app = Application::new();
    let german = app.register_language("de").unwrap();
    let components = app.register_standard_components().unwrap();
    app.register_standard_painters().unwrap();
    let definition = app
        .register_ui(UiDefinition::new(
            "test",
            LivePart::Component(ComponentPart::typed_with_expressions(
                components.label,
                vec![Expression::text("Save").unwrap()],
                label,
            )),
        ))
        .unwrap();
    let (entered, waiting) = crossbeam_channel::bounded(1);
    let (release, gate) = crossbeam_channel::bounded(1);
    let block = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reload = builder(&app)
        .catalog(
            dir.filesystem(),
            ResourcePath::new("de.po").unwrap(),
            "test",
            german,
            Arc::new(PausedFormat {
                block: block.clone(),
                entered,
                release: gate,
            }),
        )
        .unwrap()
        .start()
        .unwrap();
    reload.wait_initial(TIMEOUT).unwrap();
    let (_, outputs) = app
        .create_ui(
            definition,
            PresentationSettings {
                language: german,
                ..Default::default()
            },
        )
        .unwrap();
    let old = outputs.recv_timeout(TIMEOUT).unwrap();
    block.store(true, Ordering::Release);
    dir.write("de.po", catalog("Obsolete"));
    waiting.recv_timeout(TIMEOUT).unwrap();
    dir.write("de.po", catalog("Latest content"));
    release.send(()).unwrap();
    let frame = outputs.recv_timeout(TIMEOUT).unwrap();
    assert!(frame.display_list.commands.iter().any(
        |command| matches!(command, DrawCommand::DrawText { text, .. } if text == "Latest content")
    ));
    assert!(
        old.display_list.commands.iter().any(
            |command| matches!(command, DrawCommand::DrawText { text, .. } if text == "Initial")
        )
    );
    reload.stop();
}

#[test]
fn application_failure_stops_its_reload_session() {
    let dir = Directory::new();
    let app = Application::new();
    app.set_image_loader(ImageLoader::new(dir.filesystem()))
        .unwrap();
    let reload = builder(&app).watch_images().start().unwrap();
    reload.wait_initial(TIMEOUT).unwrap();
    assert!(
        app.inspect::<()>(|_| panic!("deliberate worker failure"))
            .is_err()
    );
    until(|| (!reload.is_running()).then_some(()));
    reload.stop();
}

#[test]
fn moving_a_directory_into_the_watch_root_reloads_registered_descendants() {
    let watched = Directory::new();
    let incoming = Directory::new();
    incoming.write("logo.png", png(42));
    let app = Application::new();
    app.set_image_loader(ImageLoader::new(watched.filesystem()))
        .unwrap();
    let reload = builder(&app).watch_images().start().unwrap();
    reload.wait_initial(TIMEOUT).unwrap();
    assert!(image(&app, "icons/logo.png").is_none());
    std::fs::rename(&incoming.0, watched.0.join("icons")).unwrap();
    // Directory's cleanup guard still owns a path; recreate that empty path.
    std::fs::create_dir(&incoming.0).unwrap();
    wait_image(&app, "icons/logo.png", 42);
    reload.stop();
}
