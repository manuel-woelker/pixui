//! Resource configuration for the todo example's core image component.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    resources::{
        directory::DirectoryFilesystem, filesystem::ResourceFilesystem, image_loader::ImageLoader,
        layered::LayeredFilesystem,
    },
};
use std::{path::Path, sync::Arc};

/// The optional override directory uses the same relative namespace
/// (`images/pixui-logo.png`) as the repository assets root. Images are loaded
/// lazily during worker-side component preparation, not in the painter.
pub fn configure_resources(
    app: &ApplicationHandle,
    override_root: Option<&Path>,
) -> PixuiResult<()> {
    let mut sources: Vec<Arc<dyn ResourceFilesystem>> = Vec::new();
    if let Some(root) = override_root {
        sources.push(Arc::new(DirectoryFilesystem::new(root)?));
    }
    sources.push(Arc::new(DirectoryFilesystem::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets"
    ))?));
    app.set_image_loader(ImageLoader::new(Arc::new(LayeredFilesystem::new(sources))))
}
