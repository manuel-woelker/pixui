//! Caller-owned watcher lifetime. Never store a session on its application worker.
use super::{
    builder::ResourceReloadBuilder,
    runner::Coordinator,
    shared::{Hint, Shared},
};
use crossbeam_channel::{Receiver, bounded};
use notify::{RecursiveMode, Watcher};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    collections::BTreeSet,
    sync::{Arc, atomic::Ordering},
    thread::JoinHandle,
    time::Duration,
};

/// Keeps native watches and a background loader alive. Drop cancels and joins;
/// in-flight synchronous decoding can delay completion. Own this on the native
/// application's main stack, not inside a callback or worker operation.
/// Stopping preserves installed translations and last published image versions.
pub struct ResourceReloadSession {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    initial: Receiver<PixuiResult<()>>,
}
impl ResourceReloadSession {
    pub(crate) fn start(builder: ResourceReloadBuilder) -> PixuiResult<Self> {
        if builder.quiet.is_zero() || builder.quiet > Duration::from_secs(60) {
            return Err(pixui_error!(
                "reload quiet period must be in (0, 60 seconds]"
            ));
        }
        let loader = if builder.images {
            Some(builder.application.reload_image_loader()?)
        } else {
            None
        };
        let mut roots = BTreeSet::new();
        if let Some(loader) = &loader {
            let image_roots = loader.filesystem().watch_roots();
            if image_roots.is_empty() {
                return Err(pixui_error!("image filesystem has no native watch roots"));
            }
            roots.extend(image_roots);
        }
        for catalog in &builder.catalogs {
            let catalog_roots = catalog.filesystem.watch_roots();
            if catalog_roots.is_empty() {
                return Err(pixui_error!("catalog filesystem has no native watch roots"));
            }
            roots.extend(catalog_roots);
        }
        if roots.is_empty() {
            return Err(pixui_error!("reload session has no watch targets"));
        }
        let (hints, receiver) = bounded(256);
        let shared = Arc::new(Shared::new(
            hints,
            super::shared::MAX_TARGETS - builder.catalogs.len(),
        ));
        let callback = shared.clone();
        let mut last_backend_error = None;
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
                Ok(event) if !matches!(event.kind, notify::EventKind::Access(_)) => {
                    if callback.hints.try_send(Hint::Event(event)).is_err() {
                        callback.rescan.store(true, Ordering::Release);
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    let message = error.to_string();
                    if last_backend_error.as_ref() != Some(&message) {
                        tracing::warn!(%error, "resource watcher error");
                        last_backend_error = Some(message);
                        callback.rescan.store(true, Ordering::Release);
                        callback.wake();
                    }
                }
            })
            .map_err(|e| pixui_error!("create resource watcher: {e}"))?;
        for root in roots {
            watcher
                .watch(&root, RecursiveMode::Recursive)
                .map_err(|e| pixui_error!("watch {}: {e}", root.display()))?;
        }
        builder.application.attach_resource_reload(
            shared.clone(),
            loader.clone(),
            builder.catalogs.clone(),
        )?;
        let (ready, initial) = bounded(1);
        let state = shared.clone();
        let thread = std::thread::Builder::new()
            .name("pixui-resource-loader".into())
            .spawn(move || {
                let _watcher = watcher;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    Coordinator::new(builder, state.clone(), loader, receiver, ready).run();
                }));
                state.stop();
                if result.is_err() {
                    tracing::warn!("resource loader stopped after a panic");
                }
            });
        match thread {
            Ok(thread) => Ok(Self {
                shared,
                thread: Some(thread),
                initial,
            }),
            Err(error) => {
                shared.stop();
                Err(pixui_error!("start resource loader: {error}"))
            }
        }
    }
    /// Wait once for targets known at startup to load/install or exhaust retries.
    /// A timeout leaves the session running; retry the wait later. A reported file
    /// error also leaves watching active, so a subsequent edit can recover.
    pub fn wait_initial(&self, timeout: Duration) -> PixuiResult<()> {
        self.initial
            .recv_timeout(timeout)
            .map_err(|e| pixui_error!("wait for initial resource loads: {e}"))?
    }
    /// Whether the coordinator is still active. Worker failure ends the session.
    pub fn is_running(&self) -> bool {
        self.shared.active()
    }
    /// Cancel queued updates and join. Never call from this application's worker.
    pub fn stop(mut self) {
        self.shutdown();
    }
    fn shutdown(&mut self) {
        self.shared.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for ResourceReloadSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}
