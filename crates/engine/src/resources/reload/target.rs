//! Concrete load jobs and prepared worker updates share scheduling and delivery.
use super::{scheduler::Schedule, shared::Shared};
use crate::{
    application::app::Application,
    i18n::{catalog::TranslationCatalog, format::TranslationFormat, indices::LanguageIndex},
    resources::{
        filesystem::ResourceFilesystem, image_loader::ImageLoader, path::ResourcePath,
        read::read_bounded,
    },
    ui::image::Image,
};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

#[derive(Clone)]
pub(crate) struct CatalogTarget {
    pub filesystem: Arc<dyn ResourceFilesystem>,
    pub path: ResourcePath,
    pub domain: String,
    pub language: LanguageIndex,
    pub format: Arc<dyn TranslationFormat + Send + Sync>,
}
#[derive(Clone)]
pub(crate) enum Source {
    Image(ImageLoader),
    Catalog(CatalogTarget),
}
pub(crate) struct Target {
    pub source: Source,
    pub path: ResourcePath,
    pub schedule: Schedule,
    pub accepted: Option<blake3::Hash>,
    pub initial: bool,
    pub last_error: Option<String>,
}
impl Source {
    pub fn filesystem(&self) -> &dyn ResourceFilesystem {
        match self {
            Self::Image(loader) => loader.filesystem().as_ref(),
            Self::Catalog(catalog) => catalog.filesystem.as_ref(),
        }
    }
    pub fn limit(&self) -> usize {
        match self {
            Self::Image(loader) => loader.encoded_limit(),
            Self::Catalog(_) => 16 * 1024 * 1024,
        }
    }
    pub fn decode(&self, bytes: &[u8]) -> PixuiResult<Payload> {
        match self {
            Self::Image(loader) => Ok(Payload::Image(loader.decode(bytes)?)),
            Self::Catalog(catalog) => Ok(Payload::Catalog(
                catalog.format.import(
                    std::str::from_utf8(bytes)
                        .map_err(|e| pixui_error!("catalog is not UTF-8: {e}"))?,
                )?,
            )),
        }
    }
}
impl Target {
    pub fn affected(&self, paths: &[PathBuf]) -> bool {
        self.source.filesystem().watch_roots().iter().any(|root| {
            let file = root.join(self.path.as_str());
            paths
                .iter()
                .any(|event| file == *event || file.starts_with(event))
        })
    }
    pub fn read(&self) -> PixuiResult<Vec<u8>> {
        read_bounded(self.source.filesystem(), &self.path, self.source.limit())
    }
    pub fn stamp(&self) -> Vec<Option<(u64, Option<SystemTime>)>> {
        self.source
            .filesystem()
            .watch_roots()
            .iter()
            .map(|root| stamp(&root.join(self.path.as_str())))
            .collect()
    }
}
fn stamp(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    std::fs::metadata(path)
        .ok()
        .map(|m| (m.len(), m.modified().ok()))
}

pub(crate) enum Payload {
    Image(Image),
    Catalog(TranslationCatalog),
}
pub(crate) struct Prepared {
    pub shared: Arc<Shared>,
    pub revision: Arc<AtomicU64>,
    pub version: u64,
    pub source: Source,
    pub path: ResourcePath,
    pub payload: Payload,
}
impl Prepared {
    /// The worker owns all application mutation; callbacks cannot install data.
    pub fn apply(self, app: &mut Application) -> PixuiResult<bool> {
        let _guard = self.shared.acceptance.lock().unwrap();
        if !self.shared.active()
            || self.revision.load(Ordering::Acquire) != self.version
            || !app
                .resource_reload
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .is_some_and(|current| Arc::ptr_eq(&current, &self.shared))
        {
            return Ok(false);
        }
        match (self.source, self.payload) {
            (Source::Image(loader), Payload::Image(image)) => {
                let service = app.image_service.get_mut();
                if !service
                    .loader()
                    .is_some_and(|current| current.same_configuration(&loader))
                {
                    return Ok(false);
                }
                service.replace(self.path.clone(), &image);
                self.shared
                    .images
                    .lock()
                    .unwrap()
                    .snapshots
                    .insert(self.path, image);
                app.uis.invalidate_all();
            }
            (Source::Catalog(catalog), Payload::Catalog(value)) => {
                let report = app.install_translations(&catalog.domain, catalog.language, value)?;
                for diagnostic in report.diagnostics {
                    tracing::warn!(domain = catalog.domain, %diagnostic, "resource catalog diagnostic");
                }
            }
            _ => unreachable!("a job prepares its own payload kind"),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{resources::filesystem::ResourceReader, ui::display_list::Color};
    struct Empty;
    impl ResourceFilesystem for Empty {
        fn open(&self, _: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
            Ok(None)
        }
    }
    fn shared() -> Arc<Shared> {
        let (sender, _) = crossbeam_channel::bounded(1);
        Arc::new(Shared::new(sender, 1))
    }
    fn prepared(shared: &Arc<Shared>, loader: &ImageLoader, revision: u64) -> Prepared {
        Prepared {
            shared: shared.clone(),
            revision: Arc::new(AtomicU64::new(revision)),
            version: 1,
            source: Source::Image(loader.clone()),
            path: ResourcePath::new("logo.png").unwrap(),
            payload: Payload::Image(Image::new(1, 1, vec![Color(1, 2, 3)], None).unwrap()),
        }
    }
    #[test]
    fn worker_rejects_stale_revision_session_loader_and_cancelled_updates() {
        let loader = ImageLoader::new(Arc::new(Empty));
        let mut app = Application::default();
        app.set_image_loader(loader.clone());
        let state = shared();
        app.resource_reload = Some(Arc::downgrade(&state));
        app.image_service.get_mut().watch(state.clone()).unwrap();
        assert!(!prepared(&state, &loader, 2).apply(&mut app).unwrap());
        let foreign = shared();
        assert!(!prepared(&foreign, &loader, 1).apply(&mut app).unwrap());
        assert!(
            !prepared(&state, &ImageLoader::new(Arc::new(Empty)), 1)
                .apply(&mut app)
                .unwrap()
        );
        assert!(state.images.lock().unwrap().snapshots.is_empty());
        assert!(prepared(&state, &loader, 1).apply(&mut app).unwrap());
        assert!(
            app.load_image(&ResourcePath::new("logo.png").unwrap())
                .is_ok()
        );
        state.stop();
        assert!(!prepared(&state, &loader, 1).apply(&mut app).unwrap());
    }
}
