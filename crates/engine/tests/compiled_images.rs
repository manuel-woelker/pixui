//! Compiled image props share normal loading, caching and frame publication.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::{app::Application, application_slice::ApplicationSlice},
    resources::{
        filesystem::{ResourceFilesystem, ResourceReader},
        image_loader::ImageLoader,
        layered::LayeredFilesystem,
        path::ResourcePath,
    },
    ui::{
        display_list::{DrawCommand, RenderOutput},
        image::Image,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
    },
    ui_language::{compiler::compile, resolution::environment::Environment},
};
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
struct Source {
    reads: Arc<AtomicUsize>,
    names: &'static [&'static str],
}
impl ResourceFilesystem for Source {
    fn open(&self, path: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
        if !self.names.contains(&path.as_str()) {
            return Ok(None);
        }
        self.reads.fetch_add(1, Ordering::SeqCst);
        let data: &[u8] = if path.as_str() == "broken.png" {
            b"not an image"
        } else {
            include_bytes!("../../../assets/images/pixui-logo.png")
        };
        Ok(Some(Box::new(Cursor::new(data))))
    }
}
#[pixui_engine::application::action::slice_actions(slice="images", facade=Actions)]
mod actions {
    #[action]
    pub fn select(
        mut filename: pixui_engine::application::entity_mut::EntityMut<String>,
        value: String,
    ) {
        *filename = value;
    }
}
fn receive(outputs: &OutputReceiver) -> RenderOutput {
    outputs.recv_timeout(Duration::from_secs(3)).unwrap()
}
#[test]
fn dynamic_paths_reuse_snapshots_retain_failed_frames_and_recover() {
    let app = Application::new();
    let mut slice = ApplicationSlice::new("images");
    slice.bind("filename", "one.png".to_owned()).unwrap();
    let slice = app.add_slice(slice).unwrap();
    actions::Actions::register(&app, slice).unwrap();
    let actions = actions::Actions::bind(&app).unwrap();
    let components = app.register_standard_components().unwrap();
    app.register_standard_painters().unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    app.set_image_loader(ImageLoader::new(Arc::new(LayeredFilesystem::new(vec![
        Arc::new(Source {
            reads: reads.clone(),
            names: &["one.png"],
        }),
        Arc::new(Source {
            reads: reads.clone(),
            names: &["two.png", "broken.png"],
        }),
    ]))))
    .unwrap();
    let definition = app
        .inspect(move |app| {
            let mut env = Environment::new();
            env.component("Image", components.image)?;
            env.entity(
                "filename",
                app,
                app.entity_ref::<String>(slice, "filename")?,
            )?;
            env.layout(
                "logo",
                pixui_engine::layout::style::LayoutStyle::fixed(160.0, 160.0),
            )?;
            Ok(compile(
                "worker-image.pixui",
                "ui Images {<Image path=entities.filename layout=layouts.logo />}",
                &env,
            )
            .unwrap()
            .definition)
        })
        .unwrap();
    let definition = app.register_ui(definition).unwrap();
    let (instance, outputs) = app
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let first = receive(&outputs);
    assert_eq!(first.display_list.images.len(), 1);
    assert!(first.display_list.commands.iter().any(|command| matches!(command, DrawCommand::DrawImage { destination, .. } if destination.width == 160.0 && destination.height == 160.0)));
    actions.select("one.png").unwrap();
    app.inspect(|_| Ok(())).unwrap();
    let same = receive(&outputs);
    assert!(Image::ptr_eq(
        &first.display_list.images[0],
        &same.display_list.images[0]
    ));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    actions.select("two.png").unwrap();
    app.inspect(|_| Ok(())).unwrap();
    let second = receive(&outputs);
    assert!(!Image::ptr_eq(
        &first.display_list.images[0],
        &second.display_list.images[0]
    ));
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    for filename in ["../bad.png", "missing.png", "broken.png"] {
        actions.select(filename).unwrap();
        app.inspect(move |app| {
            let instance = app.uis().instance(instance)?;
            assert_eq!(instance.revision(), second.revision);
            assert!(
                instance
                    .last_error()
                    .unwrap()
                    .contains("worker-image.pixui")
            );
            Ok(())
        })
        .unwrap();
        assert!(outputs.try_recv().is_err());
    }
    actions.select("two.png").unwrap();
    app.inspect(|_| Ok(())).unwrap();
    let recovered = receive(&outputs);
    assert!(Image::ptr_eq(
        &second.display_list.images[0],
        &recovered.display_list.images[0]
    ));
    assert!(recovered.revision > second.revision);
}
