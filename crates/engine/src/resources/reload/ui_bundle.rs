//! Filesystem bundle jobs use the same coordinator as images and catalogs.
use crate::{
    application::application_handle::ApplicationHandle,
    resources::{filesystem::ResourceFilesystem, path::ResourcePath, read::read_bounded},
    ui_language::{
        bundle,
        bundle_prepare::PreparedBundle,
        bundle_registration::{BundleId, ConfigurationStamp},
        source::bundle::{ModuleSource, SourceResolver, canonical},
    },
};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub(crate) struct UiTarget {
    pub filesystem: Arc<dyn ResourceFilesystem>,
    pub bundle: BundleId,
    pub paths: Arc<Mutex<BTreeSet<ResourcePath>>>,
}
pub(crate) struct UiPrepared {
    pub bundle: BundleId,
    pub revision: u64,
    pub stamp: ConfigurationStamp,
    pub value: PreparedBundle,
}
impl UiTarget {
    pub fn affected(&self, paths: &[PathBuf]) -> bool {
        let known = self.paths.lock().unwrap();
        self.filesystem.watch_roots().iter().any(|root| {
            paths.iter().any(|path| {
                path.starts_with(root)
                    && (path.extension().is_some_and(|ext| ext == "pixui")
                        || known
                            .iter()
                            .any(|name| root.join(name.as_str()).starts_with(path)))
            })
        })
    }
    pub fn stamp(&self) -> Vec<Option<super::target::FileStamp>> {
        self.paths
            .lock()
            .unwrap()
            .iter()
            .flat_map(|path| {
                self.filesystem
                    .watch_roots()
                    .iter()
                    .map(|root| super::target::stamp(&root.join(path.as_str())))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
    pub fn prepare(
        &self,
        application: &ApplicationHandle,
        accepted: Option<blake3::Hash>,
        shared: &super::shared::Shared,
    ) -> PixuiResult<(blake3::Hash, Option<super::target::Payload>)> {
        let id = self.bundle;
        let snapshot = snapshot(application, id, shared)?;
        let resolver = Files {
            filesystem: self.filesystem.as_ref(),
            limit: snapshot.limits.source_bytes,
            sources: Mutex::new(BTreeMap::new()),
            stamps: Mutex::new(BTreeMap::new()),
        };
        let result = bundle::prepare(
            &resolver,
            &snapshot.entries,
            &snapshot.environment,
            &snapshot.domain,
            snapshot.limits,
            snapshot.translations,
        );
        // Keep failed/missing paths too, so creating a missing import recovers.
        let stamps = resolver.stamps.lock().unwrap();
        *self.paths.lock().unwrap() = stamps.keys().cloned().collect();
        for (path, before) in stamps.iter() {
            if *before != file_stamps(self.filesystem.as_ref(), path) {
                return Err(pixui_error!("UI sources changed during preparation"));
            }
        }
        let value = result?;
        let mut hash = blake3::Hasher::new();
        for (name, contents) in resolver.sources.lock().unwrap().iter() {
            hash.update(&(name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update(&(contents.len() as u64).to_le_bytes());
            hash.update(contents.as_bytes());
        }
        let hash = hash.finalize();
        if accepted == Some(hash) {
            return Ok((hash, None));
        }
        Ok((
            hash,
            Some(super::target::Payload::Ui(Box::new(UiPrepared {
                bundle: id,
                revision: snapshot.revision,
                stamp: snapshot.stamp,
                value,
            }))),
        ))
    }
}
fn file_stamps(
    filesystem: &dyn ResourceFilesystem,
    path: &ResourcePath,
) -> Vec<Option<super::target::FileStamp>> {
    filesystem
        .watch_roots()
        .iter()
        .map(|root| super::target::stamp(&root.join(path.as_str())))
        .collect()
}
struct Files<'a> {
    filesystem: &'a dyn ResourceFilesystem,
    limit: usize,
    sources: Mutex<BTreeMap<String, String>>,
    stamps: Mutex<BTreeMap<ResourcePath, Vec<Option<super::target::FileStamp>>>>,
}
impl SourceResolver for Files<'_> {
    fn resolve(&self, importer: Option<&str>, specifier: &str) -> PixuiResult<ModuleSource> {
        let id = canonical(importer, specifier)?;
        if let Some(text) = self.sources.lock().unwrap().get(&id) {
            return Ok(ModuleSource {
                id: id.clone(),
                text: text.clone(),
            });
        }
        let path = ResourcePath::new(id.as_str())?;
        self.stamps
            .lock()
            .unwrap()
            .insert(path.clone(), file_stamps(self.filesystem, &path));
        let text = String::from_utf8(read_bounded(self.filesystem, &path, self.limit)?)
            .map_err(|e| pixui_error!("UI source {id} is not UTF-8: {e}"))?;
        self.sources
            .lock()
            .unwrap()
            .insert(id.clone(), text.clone());
        Ok(ModuleSource { id, text })
    }
}

fn snapshot(
    application: &ApplicationHandle,
    id: BundleId,
    shared: &super::shared::Shared,
) -> PixuiResult<crate::ui_language::bundle_registration::BundleSnapshot> {
    let reply = loop {
        if !shared.active() {
            return Err(pixui_error!("UI reload cancelled"));
        }
        match application.try_bundle_snapshot(id) {
            Ok(reply) => break reply,
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                shared.stop();
                return Err(pixui_error!("application worker stopped"));
            }
        }
    };
    loop {
        if !shared.active() {
            return Err(pixui_error!("UI reload cancelled"));
        }
        match reply.try_recv() {
            Ok(result) => return result,
            Err(crossbeam_channel::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                shared.stop();
                return Err(pixui_error!("application worker stopped"));
            }
        }
    }
}
