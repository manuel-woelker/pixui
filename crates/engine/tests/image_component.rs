//! Core image preparation, path changes, weak ownership and shared lookup.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::app::Application,
    components::image::{ImageComponent, ImageProps, ImageState},
    expression::context::ExpressionContext,
    live_model::{
        component::Component,
        part::{ComponentPart, LivePart},
        state::LiveState,
    },
    painters::image::ImagePainter,
    resources::{
        filesystem::{ResourceFilesystem, ResourceReader},
        image_loader::ImageLoader,
        path::ResourcePath,
    },
    ui::{image::Image, presentation::PresentationSettings, renderer},
};
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Source(Arc<AtomicUsize>);
impl ResourceFilesystem for Source {
    fn open(&self, path: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        if path.as_str() == "missing.png" {
            return Ok(None);
        }
        let bytes = include_bytes!("../../../assets/images/pixui-logo.png");
        Ok(Some(Box::new(Cursor::new(bytes.as_slice()))))
    }
}
fn loader(calls: &Arc<AtomicUsize>) -> ImageLoader {
    ImageLoader::new(Arc::new(Source(calls.clone())))
}

#[test]
fn preparation_reuses_paths_changes_snapshots_and_releases_pixels() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut app = Application::default();
    app.set_image_loader(loader(&calls));
    let mut one = ImageState::default();
    let mut two = ImageState::default();
    let first = ImageProps::new("one.png").unwrap();
    let second = ImageProps::new("two.png").unwrap();
    let prepare = |state: &mut ImageState, props: &ImageProps| {
        ImageComponent::prepare(props, state, &ExpressionContext::new(&app))
    };
    prepare(&mut one, &first).unwrap();
    prepare(&mut two, &first).unwrap();
    prepare(&mut one, &first).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let original = one.image().unwrap().clone();
    assert!(Image::ptr_eq(&original, two.image().unwrap()));
    prepare(&mut one, &second).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(!Image::ptr_eq(&original, one.image().unwrap()));
    assert!(prepare(&mut one, &ImageProps::new("missing.png").unwrap()).is_err());
    let second_version = one.image().unwrap().clone();
    prepare(&mut one, &second).unwrap();
    assert!(Image::ptr_eq(&second_version, one.image().unwrap()));
    drop(second_version);
    let weak = one.image().unwrap().downgrade();
    drop(one);
    assert!(weak.upgrade().is_none(), "cache must not retain pixel data");
    app.set_image_loader(loader(&calls));
    ImageComponent::prepare(&first, &mut two, &ExpressionContext::new(&app)).unwrap();
    assert!(!Image::ptr_eq(&original, two.image().unwrap()));
    // Retained outputs/resources remain valid across source replacement.
    assert!(original.width() > 0);
}

#[test]
fn normal_binding_prepares_images_before_painting_and_reports_missing_loader() {
    let mut app = Application::default();
    let id = app.register_component::<ImageComponent>("image").unwrap();
    app.register_painter::<ImageComponent>(ImagePainter)
        .unwrap();
    let template = LivePart::Component(ComponentPart::typed(id, |_, _| ImageProps::new("one.png")));
    let settings = PresentationSettings::default();
    let mut state = LiveState::new();
    let error = renderer::render(&template, &mut state, &app, &settings, 0.0, None, None)
        .err()
        .unwrap();
    assert!(error.to_string().contains("no application image loader"));
    let calls = Arc::new(AtomicUsize::new(0));
    app.set_image_loader(loader(&calls));
    let first = renderer::render(&template, &mut state, &app, &settings, 0.0, None, None)
        .unwrap()
        .0;
    let next = renderer::render(&template, &mut state, &app, &settings, 0.0, None, None)
        .unwrap()
        .0;
    assert_eq!(first.images.len(), 1);
    assert!(Image::ptr_eq(&first.images[0], &next.images[0]));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "painting must perform no extra reads"
    );
}
