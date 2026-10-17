//! Detached graphs, atomic publication, and stable references without a filesystem.
use super::{
    bundle::{self, BundleLimits},
    bundle_prepare::PreparedBundle,
    bundle_registration::{BundleId, BundleSnapshot},
    resolution::environment::Environment,
    source::bundle::SourceBundle,
};
use crate::{
    application::app::Application,
    components::label::LabelComponent,
    painters::label::LabelPainter,
    styles::{family::StyleFamily, style::PaintStyle},
};
fn fixture() -> (Application, Environment) {
    let mut app = Application::default();
    let label = app.register_component::<LabelComponent>("label").unwrap();
    app.register_painter::<LabelComponent>(LabelPainter::default())
        .unwrap();
    let mut env = Environment::new();
    env.component("Label", label).unwrap();
    (app, env)
}
fn sources(main: &str, library: &str) -> SourceBundle {
    let mut sources = SourceBundle::new();
    sources.add("main.pixui", main).unwrap();
    sources.add("common.pixui", library).unwrap();
    sources
}
fn prepare(
    app: &Application,
    id: BundleId,
    sources: &SourceBundle,
) -> (BundleSnapshot, PreparedBundle) {
    let snapshot = app.bundle_snapshot(id).unwrap();
    let value = bundle::prepare(
        sources,
        &snapshot.entries,
        &snapshot.environment,
        &snapshot.domain,
        snapshot.limits,
        snapshot.translations.clone(),
    )
    .unwrap();
    (snapshot, value)
}
const MAIN: &str = "use {Title} from './common.pixui'; ui Main {<Title text=\"Heading\" />}";
const LIB: &str = "export component Title(text: String) {<Label text=text />} export component Spare() {<Label text='spare' />}";
#[test]
fn stable_exports_tombstones_signature_rejection_and_no_slot_growth() {
    let (mut app, env) = fixture();
    let initial = bundle::install(
        &mut app,
        &sources(MAIN, LIB),
        &["main.pixui".into()],
        &env,
        "test",
        BundleLimits::default(),
    )
    .unwrap();
    let id = initial.id();
    let root = initial.component("main.pixui").unwrap();
    let spare = initial
        .export("common.pixui", "Spare")
        .unwrap()
        .reference()
        .unwrap();
    let count = app.template_count();
    for version in 0..8 {
        let library =
            format!("export component Title(text: String) {{<Label text='{{text}} {version}' />}}");
        let (snapshot, prepared) = prepare(&app, id, &sources(MAIN, &library));
        assert!(
            app.accept_bundle(id, snapshot.revision, snapshot.stamp, prepared)
                .unwrap()
        );
        assert_eq!(app.template_count(), count);
        assert_eq!(
            app.installed_bundle(id)
                .unwrap()
                .component("main.pixui")
                .unwrap(),
            root
        );
        assert!(app.clone_template(spare).is_err());
    }
    let (snapshot, prepared) = prepare(&app, id, &sources(MAIN, LIB));
    assert!(
        app.accept_bundle(id, snapshot.revision, snapshot.stamp, prepared)
            .unwrap()
    );
    assert_eq!(
        app.installed_bundle(id)
            .unwrap()
            .export("common.pixui", "Spare")
            .unwrap()
            .reference()
            .unwrap(),
        spare
    );
    assert!(app.clone_template(spare).is_ok());
    // Keep the library reachable while changing its public signature.
    let incompatible = sources(
        "use {Title} from './common.pixui'; ui Main {<Title text=true />}",
        "export component Title(text: bool) {<></>}",
    );
    let (snapshot, prepared) = prepare(&app, id, &incompatible);
    let revision = app.templates.revision();
    let translations = app.translations.revision();
    assert!(
        app.accept_bundle(id, snapshot.revision, snapshot.stamp, prepared)
            .is_err()
    );
    assert_eq!(app.templates.revision(), revision);
    assert_eq!(app.translations.revision(), translations);
    assert!(app.clone_template(spare).is_ok());
}
#[test]
fn detached_preparation_is_send_and_obsolete_configuration_never_commits() {
    fn send<T: Send>() {}
    send::<PreparedBundle>();
    send::<BundleSnapshot>();
    let (mut app, mut env) = fixture();
    let calls = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let record = calls.clone();
    let original = env.components.get_mut("Label").unwrap();
    let build = original.build.clone();
    original.build = std::sync::Arc::new(move |types| {
        record.lock().unwrap().push(
            std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .to_owned(),
        );
        build(types)
    });
    let installed = bundle::install(
        &mut app,
        &sources(MAIN, LIB),
        &["main.pixui".into()],
        &env,
        "test",
        BundleLimits::default(),
    )
    .unwrap();
    let id = installed.id();
    calls.lock().unwrap().clear();
    let snapshot = app.bundle_snapshot(id).unwrap();
    let old = snapshot.stamp;
    let version = snapshot.revision;
    let prepared = std::thread::Builder::new()
        .name("ui-test-loader".into())
        .spawn(move || {
            bundle::prepare(
                &sources(MAIN, LIB),
                &snapshot.entries,
                &snapshot.environment,
                &snapshot.domain,
                snapshot.limits,
                snapshot.translations,
            )
            .unwrap()
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(!calls.lock().unwrap().is_empty());
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .all(|name| name == "ui-test-loader")
    );
    app.register_style("new", StyleFamily::new(PaintStyle::text))
        .unwrap();
    let count = app.template_count();
    assert!(!app.accept_bundle(id, version, old, prepared).unwrap());
    assert_eq!(app.template_count(), count);
    let (snapshot, first) = prepare(&app, id, &sources(MAIN, LIB));
    let (_, second) = prepare(&app, id, &sources(MAIN, LIB));
    assert!(
        app.accept_bundle(id, snapshot.revision, snapshot.stamp, first)
            .unwrap()
    );
    assert!(
        !app.accept_bundle(id, snapshot.revision, snapshot.stamp, second)
            .unwrap()
    );
    let (other, _) = fixture();
    assert!(other.installed_bundle(id).is_err());
    assert!(other.bundle_snapshot(id).is_err());
}
#[test]
fn library_only_replacement_of_a_configured_entry_is_rejected_atomically() {
    let (mut app, env) = fixture();
    let installed = bundle::install(
        &mut app,
        &sources(MAIN, LIB),
        &["main.pixui".into()],
        &env,
        "test",
        BundleLimits::default(),
    )
    .unwrap();
    let id = installed.id();
    let (snapshot, prepared) = prepare(&app, id, &sources("export component Empty() {<></>}", LIB));
    let before = app.templates.revision();
    assert!(
        app.accept_bundle(id, snapshot.revision, snapshot.stamp, prepared)
            .is_err()
    );
    assert_eq!(app.templates.revision(), before);
}
